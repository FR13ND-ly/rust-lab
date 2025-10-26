use anyhow::{Result, bail};
use std::{
    fs::{self, File},
    io::Write,
};

fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}

fn blue(s: &str) -> String {
    format!("\x1b[34m{}\x1b[0m", s)
}

fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

// 1
fn longest(file_path: &str) -> Result<(usize, usize)> {
    let s = fs::read_to_string(file_path)?;

    let mut chars_max_len: usize = 0;
    let mut bytes_max_len: usize = 0;

    use std::cmp::*;
    for line in s.lines() {
        bytes_max_len = max(bytes_max_len, line.len());
        chars_max_len = max(chars_max_len, line.chars().count());
    }

    Ok((chars_max_len, bytes_max_len))
}

// 2
fn rot13(file_path: &str) -> Result<String> {
    let s = fs::read_to_string(file_path)?;
    if !s.is_ascii() {
        bail!("Non ASCII string!");
    }
    let result: String = s
        .chars()
        .map(|c| match c {
            'a'..='m' | 'A'..='M' => (c as u8 + 13u8) as char,
            'n'..='z' | 'N'..='Z' => (c as u8 - 13u8) as char,
            _ => c,
        })
        .collect();
    Ok(result)
}

// 3
fn abbreviations(file_path: &str) -> Result<String> {
    let s = fs::read_to_string(file_path)?;
    let mut result = String::new();
    for word in s.split_whitespace() {
        let word_to_push = match word {
            "pt" | "ptr" => "pentru",
            "dl" => "domnul",
            "dna" => "doamna",
            _ => word,
        };
        result.push_str(word_to_push);
        result.push(' ');
    }
    result.pop();
    Ok(result)
}

// 4
fn hosts(file_path: &str) -> Result<()> {
    let s = fs::read_to_string(file_path)?;
    for line in s.lines() {
        if line.starts_with("#") {
            continue;
        }
        let mut it = line.split_whitespace();
        if let Some(host) = it.next() {
            if let Some(hostname) = it.next() {
                println!("{} => {}", bold(&hostname), green(&host))
            }
        }
    }

    Ok(())
}

fn bonus_rot13(file_path: &str) -> Result<String> {
    let s = fs::read_to_string(file_path)?;
    if !s.is_ascii() {
        bail!("Non ASCII string!");
    }

    let v: Vec<u8> = s
        .bytes()
        .map(|i| {
            if b'a' <= i && i <= b'm' || b'A' <= i && i <= b'M' {
                return i + 13;
            } else if b'n' <= i && i <= b'z' || b'N' <= i && i <= b'Z' {
                return i - 13;
            }
            i
        })
        .collect();

    Ok(String::from_utf8(v)?)
}

fn bonus() -> Result<()> {
    let mut file = File::create("bonus.txt")?;

    let text = b"This is a string to the file!";
    let mut size: usize = 0;
    let target = 4 * 1024 * 1024 * 1024;

    println!("Creating the file...");

    while size < target {
        file.write_all(text)?;
        size += text.len();
    }

    if bonus_rot13("bonus.txt").is_ok() {
        println!("ROT13 encryption done");
    }

    Ok(())
}

fn main() {
    println!("{}", bold(&blue("\nEx.1")));
    match longest("ex1.txt") {
        Ok((bytes, chars)) => println!("Bytes: {}, Chars: {}", green(&bytes.to_string()), green(&chars.to_string())),
        Err(err) => println!("{}", red(&err.to_string())),
    }

    println!("{}", bold(&blue("\nEx.2")));
    match rot13("ex2.txt") {
        Ok(s) => println!("{s}"),
        Err(err) => println!("{}", red(&err.to_string())),
    }

    println!("{}", bold(&blue("\nEx.3")));
    match abbreviations("ex3.txt") {
        Ok(s) => println!("{s}"),
        Err(err) => println!("{}", red(&err.to_string())),
    }
    println!("{}", bold(&blue("\nEx.4")));
    match hosts("C:/Windows/System32/drivers/etc/hosts") {
        Ok(_) => (),
        Err(err) => println!("{}", red(&err.to_string())),
    }

    //Bonus
    println!("{}", bold(&blue("\nBonus")));

    match bonus() {
        Ok(_) => (),
        Err(err) => println!("{}", red(&err.to_string())),
    }
}
