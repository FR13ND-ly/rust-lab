use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(required = true, num_args = 1..)]
    locations: Vec<String>,
}

#[tokio::main]
async fn main() {
let args = Args::parse();
    println!("Client Starting...");
    println!("Syncing Locations: {:?}", args.locations);

}