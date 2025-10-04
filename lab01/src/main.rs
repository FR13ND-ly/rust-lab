fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    for i in 2..n {
        if n % i == 0 {
            return false;
        }
    }
    true
}

fn primes_0_to_100() {
    println!("Primes from 0 to 100:");
    for n in 0..=100 {
        if is_prime(n) {
            print!("{} ", n);
        }
    }
    println!("\n");
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a
}

fn coprimes_0_to_100() {
    println!("Coprime pairs between 0 and 100:");
    for a in 0..=100 {
        for b in 0..=100 {
            if gcd(a, b) == 1 {
                println!("({}, {})", a, b);
            }
        }
    }
    println!();
}

fn bottles_of_beer() {
    for n in (1..=99).rev() {
        let bottle = if n == 1 { "bottle" } else { "bottles" };
        let next = n - 1;

        println!("{n} {bottle} of beer on the wall,");
        println!("{n} {bottle} of beer.");
        println!("Take one down, pass it around,");

        if next > 0 {
            let next_bottle = if next == 1 { "bottle" } else { "bottles" };
            println!("{next} {next_bottle} of beer on the wall.\n");
        } else {
            println!("No bottles of beer on the wall.\n");
        }
    }
}

fn main() {
    primes_0_to_100();
    coprimes_0_to_100();
    bottles_of_beer();
}
