use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    debug_data: DwarfData,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    breakpoints: Vec<usize>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };
        debug_data.print();

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            debug_data,
            history_path,
            readline,
            inferior: None,
            breakpoints: Vec::new(),
        }
    }

    fn kill(&mut self) {
        if let Some(inferior) = self.inferior.take() {
            inferior.kill();
        }
    }

    fn cont(&mut self) {
        match &mut self.inferior {
            Some(inferior) => {
                println!("Continuing.");
                match inferior.cont(&self.breakpoints) {
                    Ok(status) => match status {
                        Status::Stopped(signal, rip) => {
                            println!("Child stopped (signal {signal})");
                            if let Some(line) = self.debug_data.get_line_from_addr(rip - 1) {
                                println!("Stopped at {line}",);
                            } else {
                                println!("Stopped at 0x{:x}", rip - 1);
                            }
                        }
                        Status::Exited(status) => {
                            println!("Child exited (status {status})");
                            self.inferior = None;
                        }
                        Status::Signaled(signal) => {
                            println!("\nProgram terminated with signal {signal}, Killed.");
                            println!("The program no longer exists.");
                            self.inferior = None;
                        }
                    },
                    Err(err) => {
                        println!();
                        println!("{err}");
                        println!("Command aborted.");
                        return;
                    }
                }
            }
            None => println!("The program is not being run."),
        }
    }

    fn set_breakpoint(&mut self, addr: usize) {
        println!("Set breakpoint {} at 0x{addr:x}", self.breakpoints.len());
        self.breakpoints.push(addr);
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    // Kill any existing inferiors before starting new ones
                    self.kill();

                    // Create the inferior
                    if let Some(inferior) = Inferior::new(&self.target, &args) {
                        self.inferior = Some(inferior);
                        self.cont();
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Continue => self.cont(),
                DebuggerCommand::Backtrace => match &self.inferior {
                    Some(inferior) => inferior.print_backtrace(&self.debug_data).unwrap(),
                    None => println!("The program is not being run."),
                },
                DebuggerCommand::Break(arg) => match arg {
                    Some(arg) => {
                        if arg.as_bytes()[0] == b'*' {
                            let addr = Self::parse_address(&arg[1..]);
                            match addr {
                                Some(addr) => self.set_breakpoint(addr),
                                None => println!("Invalid hex number \"{}\"", &arg[1..]),
                            }
                        } else {
                            if let Ok(line_number) = arg.parse::<usize>() {
                                match self.debug_data.get_addr_for_line(None, line_number) {
                                    Some(addr) => self.set_breakpoint(addr),
                                    None => {
                                        println!(
                                            "No line {line_number} in file \"{}.c\".",
                                            self.target
                                        )
                                    }
                                }
                            } else {
                                match self.debug_data.get_addr_for_function(None, &arg) {
                                    Some(addr) => self.set_breakpoint(addr),
                                    None => println!("Function \"{arg}\" not defined."),
                                }
                            }
                        }
                    }
                    None => println!("No default breakpoint address now."),
                },
                DebuggerCommand::Quit => {
                    self.kill();
                    return;
                }
            }
        }
    }

    fn parse_address(addr: &str) -> Option<usize> {
        let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
            &addr[2..]
        } else {
            &addr
        };
        usize::from_str_radix(addr_without_0x, 16).ok()
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}
