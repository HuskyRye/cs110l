use crossbeam_channel;
use std::{
    thread,
    time::{self, Instant},
};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    for _ in 0..input_vec.len() {
        output_vec.push(Default::default());
    }

    let (input_sender, input_receiver) = crossbeam_channel::unbounded();
    let (output_sender, output_receiver) = crossbeam_channel::unbounded();

    let mut threads = Vec::with_capacity(num_threads);

    for _ in 0..num_threads {
        let input_receiver = input_receiver.clone();
        let output_sender = output_sender.clone();
        let handle = thread::spawn(move || {
            while let Ok((index, input)) = input_receiver.recv() {
                let output = f(input);
                output_sender.send((index, output)).unwrap();
            }
        });
        threads.push(handle);
    }
    drop(output_sender);

    while let Some(input) = input_vec.pop() {
        input_sender.send((input_vec.len(), input)).unwrap();
    }
    drop(input_sender);

    while let Ok((index, output)) = output_receiver.recv() {
        output_vec[index] = output;
    }

    for thread in threads {
        thread.join().unwrap();
    }

    output_vec
}

fn main() {
    let v = vec![6; 100];
    let v2 = v.clone();
    let start = Instant::now();
    let squares = parallel_map(v, 10, |num| {
        // println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("parallel_map finished in {:?}", start.elapsed());

    let start = Instant::now();
    let squares2: Vec<i32> = v2
        .into_iter()
        .map(|num| {
            thread::sleep(time::Duration::from_millis(500));
            num * num
        })
        .collect();
    println!("map finished in {:?}", start.elapsed());
}
