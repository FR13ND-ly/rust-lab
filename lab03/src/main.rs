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
fn next_prime(x: u16) -> Option<u16> {
    if x < 2 {
        return None;
    }

    let mut num = x;

    while num < u16::MAX {
        num += 1;
        let mut is_prime = true;
        for i in 2..=((num as f64).sqrt() as u16) {
            if num % i == 0 {
                is_prime = false;
                break;
            }
        }
        if is_prime {
            return Some(num);
        }
    }

    None
}

// 2
fn addition(a: u32, b: u32) -> u32 {
    if a > u32::MAX - b {
        panic!("Overflow");
    }
    a + b
}

fn multiplication(a: u32, b: u32) -> u32 {
    if a != 0 && b > u32::MAX / a {
        panic!("Overflow");
    }
    a * b
}

// 3
#[derive(Debug)]
enum OverflowError {
    AdditionOverflow,
    MultiplicationOverflow,
}

fn addition_o(a: u32, b: u32) -> Result<u32, OverflowError> {
    if a > u32::MAX - b {
        return Err(OverflowError::AdditionOverflow);
    }
    Ok(a + b)
}

fn multiplication_o(a: u32, b: u32) -> Result<u32, OverflowError> {
    if a != 0 && b > u32::MAX / a {
        return Err(OverflowError::MultiplicationOverflow);
    }
    Ok(a * b)
}

fn check_addition(a: u32, b: u32) -> Result<u32, OverflowError> {
    let add_result = addition_o(a, b)?;
    Ok(add_result)
}

fn check_multiplication(a: u32, b: u32) -> Result<u32, OverflowError> {
    let mul_result = multiplication_o(a, b)?;
    Ok(mul_result)
}

// 4
#[derive(Debug)]
enum CharError {
    ToUpperCase,
    ToLowerCase,
    Print,
    CharToNumber,
    CharToNumberHex,
}

fn to_uppercase(c: char) -> Result<char, CharError> {
    if !c.is_ascii_alphabetic() {
        return Err(CharError::ToUpperCase);
    }
    Ok(c.to_ascii_uppercase())
}

fn to_lowercase(c: char) -> Result<char, CharError> {
    if !c.is_ascii_alphabetic() {
        return Err(CharError::ToLowerCase);
    }
    Ok(c.to_ascii_lowercase())
}

fn print_char(c: char) -> Result<(), CharError> {
    if !c.is_ascii_graphic() && c != ' ' {
        return Err(CharError::Print);
    }
    print!("{}", c);
    Ok(())
}

fn char_to_number(c: char) -> Result<u8, CharError> {
    if !c.is_ascii_digit() {
        return Err(CharError::CharToNumber);
    }
    Ok(c as u8 - b'0')
}

fn char_to_number_hex(c: char) -> Result<u8, CharError> {
    if c.is_ascii_digit() {
        return Ok(c as u8 - b'0');
    } else if c.is_ascii_hexdigit() {
        let lower_c = c.to_ascii_lowercase();
        return Ok(lower_c as u8 - b'a' + 10);
    }
    Err(CharError::CharToNumberHex)
}

fn print_error(e: &CharError) {
    match e {
        CharError::ToUpperCase => println!(
            "{}: Character is not an alphabetic letter for to_uppercase.",
            red("Error")
        ),
        CharError::ToLowerCase => println!(
            "{}: Character is not an alphabetic letter for to_lowercase.",
            red("Error")
        ),
        CharError::Print => println!("{}: Character is not printable.", red("Error")),
        CharError::CharToNumber => println!("{}: Character is not a decimal digit.", red("Error")),
        CharError::CharToNumberHex => {
            println!("{}: Character is not a hexadecimal digit.", red("Error"))
        }
    }
}

// 5
enum Unit {
    Celsius,
    Fahrenheit,
    Kilometer,
    Mile,
}

fn text_to_unit(s: &str) -> Option<Unit> {
    match s.to_lowercase().as_str() {
        "celsius" => Some(Unit::Celsius),
        "fahrenheit" => Some(Unit::Fahrenheit),
        "kilometer" => Some(Unit::Kilometer),
        "mile" => Some(Unit::Mile),
        _ => None,
    }
}

#[derive(Debug)]
enum ConversionError {
    IncompatibleUnits,
}

fn convert(value: f64, from: Unit, to: Unit) -> Result<f64, ConversionError> {
    match (from, to) {
        (Unit::Celsius, Unit::Fahrenheit) => Ok(value * 9.0 / 5.0 + 32.0),
        (Unit::Fahrenheit, Unit::Celsius) => Ok((value - 32.0) * 5.0 / 9.0),
        (Unit::Kilometer, Unit::Mile) => Ok(value * 0.621371),
        (Unit::Mile, Unit::Kilometer) => Ok(value / 0.621371),
        _ => Err(ConversionError::IncompatibleUnits),
    }
}

fn test_convert(input: &str, from_str: &str, to_str: &str) {
    let value: f64 = match input.parse() {
        Ok(v) => v,
        Err(_) => {
            println!("{}: Invalid input value '{}'.", red("Error"), input);
            return;
        }
    };

    let from_unit = match text_to_unit(from_str) {
        Some(u) => u,
        None => {
            println!("{}: Unknown unit '{}'.", red("Error"), from_str);
            return;
        }
    };

    let to_unit = match text_to_unit(to_str) {
        Some(u) => u,
        None => {
            println!("{}: Unknown unit '{}'.", red("Error"), to_str);
            return;
        }
    };

    match convert(value, from_unit, to_unit) {
        Ok(result) => println!(
            "{} {} is {} {}",
            green(&format!("{:.2}", value)),
            from_str,
            green(&result.to_string()),
            to_str
        ),
        Err(e) => println!("{}: Error during conversion: {:?}", red("Error"), e),
    }
}

fn main() {
    println!("{}", bold(&blue("\nEx.1")));
    let mut x = 2;
    while let Some(p) = next_prime(x) {
        println!("Next prime after {}: {}", x, green(&p.to_string()));
        x = p;
    }

    println!("{}", bold(&blue("\nEx.2")));
    println!("5 + 10 = {}", addition(5, 10));
    println!("5 * 10 = {}", multiplication(5, 10));
    // addition(u32::MAX, 1);

    println!("{}", bold(&blue("\nEx.3")));
    match check_addition(10, 20) {
        Ok(v) => println!("{}: {}", green("Addition OK"), v),
        Err(e) => println!("{}: {:?}", red("Error"), e),
    }
    match check_addition(u32::MAX, 1) {
        Ok(v) => println!("{}: {}", green("Addition OK"), v),
        Err(e) => println!("{}: {:?}", red("Error"), e),
    }

    match check_multiplication(1000, 2000) {
        Ok(v) => println!("{}: {}", green("Multiplication OK"), v),
        Err(e) => println!("{}: {:?}", red("Error"), e),
    }
    match check_multiplication(u32::MAX, 2) {
        Ok(v) => println!("{}: {}", green("OK"), v),
        Err(e) => println!("{}: {:?}", red("Error"), e),
    }

    println!("{}", bold(&blue("\nEx.4")));
    match to_uppercase('a') {
        Ok(c) => println!("Uppercase: {}", c),
        Err(e) => print_error(&e),
    }
    match to_uppercase('1') {
        Ok(c) => println!("Uppercase: {}", c),
        Err(e) => print_error(&e),
    }

    match to_lowercase('B') {
        Ok(c) => println!("Lowercase: {}", c),
        Err(e) => print_error(&e),
    }

    match print_char('A') {
        Ok(()) => println!(" (printed OK)"),
        Err(e) => print_error(&e),
    }
    match print_char('\n') {
        Ok(()) => println!(" (printed OK)"),
        Err(e) => print_error(&e),
    }

    match char_to_number('5') {
        Ok(n) => println!("Char to number: {}", n),
        Err(e) => print_error(&e),
    }
    match char_to_number('x') {
        Ok(n) => println!("Char to number: {}", n),
        Err(e) => print_error(&e),
    }

    match char_to_number_hex('F') {
        Ok(n) => println!("Char to number hex: {}", n),
        Err(e) => print_error(&e),
    }
    match char_to_number_hex('z') {
        Ok(n) => println!("Char to number hex: {}", n),
        Err(e) => print_error(&e),
    }

    println!("{}", bold(&blue("\nEx.5")));
    test_convert("100", "Celsius", "Fahrenheit"); // success
    test_convert("10", "Kilometer", "Mile"); // success
    test_convert("100", "Celsius", "Mile"); // fail
    test_convert("abc", "Mile", "Kilometer"); // invalid input
    test_convert("10", "unknown", "Mile"); // invalid unit
}
