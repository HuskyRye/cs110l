// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fs;
use std::io;
use std::io::Write;
use std::iter::zip;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    // Uncomment for debugging:
    println!("random word: {secret_word}");

    // Your code here! :)
    println!("Welcome to CS110L Hangman!");

    let mut word_so_far = vec![false; secret_word_chars.len()];
    let mut guesses_left = NUM_INCORRECT_GUESSES;
    let mut guessed = String::new();

    loop {
        print!("The word so far is ");

        for (&guessed, &c) in zip(&word_so_far, &secret_word_chars) {
            print!("{}", if guessed { c } else { '-' });
        }

        println!("\nYou have guessed the following letters: {guessed}");
        println!("You have {guesses_left} guesses left");

        let ch: char = loop {
            print!("Please guess a letter: ");
            io::stdout().flush().expect("Error flushing stdout.");
            let mut guess = String::new();
            io::stdin()
                .read_line(&mut guess)
                .expect("Error reading line.");
            if let Some(c) = guess.chars().nth(0) {
                break c;
            }
        };
        guessed.push(ch);

        let mut not_in = true;
        for (guessed, &c) in zip(&mut word_so_far, &secret_word_chars) {
            if c == ch {
                *guessed = true;
                not_in = false;
            }
        }

        if not_in {
            println!("Sorry, that letter is not in the word");
            guesses_left -= 1;
            println!();
            if guesses_left == 0 {
                println!("Sorry, you ran out of guesses!");
                break;
            }
        } else {
            println!();
            let win = word_so_far.iter().all(|&guessed| guessed);
            if win {
                println!("Congratulations you guessed the secret word: {secret_word}!");
                break;
            }
        }
    }
}
