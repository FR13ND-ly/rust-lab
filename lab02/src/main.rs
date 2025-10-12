fn add_chars_n_ex1(mut s: String, c: char, number: u32) -> String {
    for _i in 0..number {
        s.push(c);
    }
    s
}
fn ex1() {
    let mut s = String::from("");
    let mut i = 0;
    while i < 26 {
        let c = (i as u8 + b'a') as char;
        s = add_chars_n_ex1(s, c, 26 - i);

        i += 1;
    }

    print!("{}", s);
}
fn add_chars_n(s: &mut String, c: char, n: usize) {
    for _i in 0..n {
        s.push(c);
    }
}
fn ex2() {
    let mut s = String::from("");
    let mut i = 0;
    while i < 26 {
        let c = (i as u8 + b'a') as char;
        add_chars_n(&mut s, c, 26 - i);
        i += 1;
    }
    print!("{}", s);
}
fn add_space(s: &mut String, n: usize) {
    for _i in 0..n {
        s.push(' ');
    }
}
fn add_str(s1: &mut String, s2: &str) {
    s1.push_str(s2);
}
fn itos(mut num: u32) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let mut result = String::new();
    let mut count = 0;

    while num > 0 {
        if count == 3 {
            result.push('_');
            count = 0;
        }

        let digit = (num % 10) as u8 + b'0';
        result.push(digit as char);

        num /= 10;
        count += 1;
    }

    result.chars().rev().collect()
}
fn add_integer(s: &mut String, num: u32) {
    let num_s = itos(num);
    s.push_str(&num_s);
}
fn add_float(s: &mut String, num: f32) {
    let integer_part = num as u32;
    let decimal_part = ((num - integer_part as f32) * 1000.0) as u32;
    add_integer(s, integer_part);
    s.push('.');
    add_integer(s, decimal_part);
}
fn ex3() {
    println!();
    let mut s = String::new();

    add_space(&mut s, 20);
    add_str(&mut s, "I💚\n");

    add_space(&mut s, 20);
    add_str(&mut s, "RUST.");
    add_str(&mut s, "\n\n");

    add_space(&mut s, 2);
    add_str(&mut s, "Most");
    add_space(&mut s, 6);
    add_str(&mut s, "crate");
    add_space(&mut s, 3);
    add_integer(&mut s, 306437968);
    add_space(&mut s, 6);
    add_str(&mut s, "and");
    add_space(&mut s, 2);
    add_str(&mut s, "lastest");
    add_space(&mut s, 4);
    add_str(&mut s, "is");
    add_str(&mut s, "\n");

    add_space(&mut s, 4);
    add_str(&mut s, "downloaded");
    add_space(&mut s, 4);
    add_str(&mut s, "has");
    add_space(&mut s, 6);
    add_str(&mut s, "downloads");
    add_space(&mut s, 2);
    add_str(&mut s, "the");
    add_space(&mut s, 4);
    add_str(&mut s, "version");
    add_space(&mut s, 2);
    add_float(&mut s, 2.038);
    add_str(&mut s, ".");

    print!("{}", s);
}

fn main() {
    ex1();
    println!("\n");
    ex2();
    println!("\n");
    ex3();
}
