use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;

use crate::dwarf_data::DwarfData;

#[derive(Debug)]
pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
    stopped: Option<usize>,
    orig_bytes: HashMap<usize, u8>,
}

use std::mem::size_of;

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

impl Inferior {
    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>) -> Option<Inferior> {
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(|| child_traceme());
        }
        let inferior = Inferior {
            child: cmd.spawn().ok()?,
            stopped: None,
            orig_bytes: HashMap::new(),
        };

        // Wait until the child process has started and paused with the signal SIGTRAP.
        if let WaitStatus::Stopped(_, signal::Signal::SIGTRAP) =
            waitpid(inferior.pid(), None).ok()?
        {
            Some(inferior)
        } else {
            None
        }
    }

    pub fn cont(&mut self, breakpoints: &Vec<usize>) -> Result<Status, nix::Error> {
        let pid = self.pid();

        // Resume execution and insert breakpoint again.
        if let Some(breakpoint) = self.stopped {
            ptrace::step(pid, None)?;
            let status = self.wait(None)?;
            if let Status::Stopped(signal::Signal::SIGTRAP, _) = status {
                self.write_byte(breakpoint, 0xcc).unwrap();
            } else {
                return Ok(status);
            }
            self.stopped = None;
        }

        // Insert breakpoints by replacing the byte at breakpoint with the value 0xcc.
        for (index, &breakpoint) in breakpoints.iter().enumerate() {
            match self.write_byte(breakpoint, 0xcc) {
                Ok(orig_byte) => {
                    self.orig_bytes.insert(breakpoint, orig_byte);
                }
                Err(err) => {
                    println!("Warning:");
                    println!("Cannot insert breakpoint {index}");
                    println!("Cannot access memory at address 0x{breakpoint:x}");
                    return Err(err);
                }
            }
        }

        // Replace 0xcc with the original byte and rewind %rip.
        let _ = ptrace::cont(pid, None);
        let result = self.wait(None)?;
        if let Status::Stopped(_, rip) = result {
            for &breakpoint in breakpoints {
                if breakpoint == rip - 1 {
                    self.write_byte(breakpoint, *self.orig_bytes.get(&breakpoint).unwrap())
                        .unwrap();
                    let mut regs = ptrace::getregs(pid)?;
                    regs.rip -= 1;
                    ptrace::setregs(pid, regs)?;
                    self.stopped = Some(breakpoint);
                    break;
                }
            }
        }
        Ok(result)
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let pid = self.pid();
        let regs = ptrace::getregs(pid)?;
        let mut rip: usize = regs.rip as usize;
        let mut rbp: usize = regs.rbp as usize;
        loop {
            let function = debug_data.get_function_from_addr(rip).unwrap();
            println!(
                "{} ({})",
                function,
                debug_data.get_line_from_addr(rip).unwrap()
            );
            if function == "main" {
                break;
            }
            rip = ptrace::read(pid, (rbp + 8) as ptrace::AddressType)? as usize;
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }
        Ok(())
    }

    pub fn kill(mut self) {
        println!("Killing running inferior (pid {})", self.pid());
        self.child.kill().unwrap();
        self.wait(None).unwrap();
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }
}
