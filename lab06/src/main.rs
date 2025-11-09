use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

use rusqlite::{Connection, Result as SqlResult};

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
trait Command {
    fn get_name(&self) -> &str;
    fn exec(&mut self, args: &[&str]);
}


struct PingCommand {}
impl Command for PingCommand {
    fn get_name(&self) -> &str {
        "ping"
    }
    fn exec(&mut self, _args: &[&str]) {
        println!("pong");
    }
}


struct CountCommand {}
impl Command for CountCommand {
    fn get_name(&self) -> &str {
        "count"
    }
    fn exec(&mut self, args: &[&str]) {
        println!("counted {} args", green(&args.len().to_string()));
    }
}


struct TimesCommand {
    count: u32,
}
impl Command for TimesCommand {
    fn get_name(&self) -> &str {
        "times"
    }
    fn exec(&mut self, _args: &[&str]) {
        self.count += 1;
        println!("'times' called {} times", green(&self.count.to_string()));
    }
}


struct EchoCommand {}
impl Command for EchoCommand {
    fn get_name(&self) -> &str {
        "echo"
    }
    fn exec(&mut self, args: &[&str]) {
        println!("{}", args.join(" "));
    }
}


// Bonus
struct BookmarkCommand {
    conn: Connection,
}

impl BookmarkCommand {
    fn new(db_path: &str) -> Result<BookmarkCommand, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bookmarks (name TEXT, url TEXT);",
            (),
        )?;

        Ok(BookmarkCommand { conn })
    }

    fn add(&self, name: &str, url: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO bookmarks (name, url) VALUES (?1, ?2);",
            (name, url),
        )?;
        Ok(())
    }
    
    fn search(&self, query: &str) -> SqlResult<()> {
        let mut stmt = self.conn.prepare(
            "SELECT name, url FROM bookmarks WHERE name LIKE ?1"
        )?;
        
        let search_query = format!("%{}%", query);

        struct Bookmark {
            name: String,
            url: String,
        }

        let bookmarks = stmt.query_map([search_query], |row| {
            Ok(Bookmark {
                name: row.get(0)?,
                url: row.get(1)?,
            })
        })?;

        let mut found_any = false;
        for bk in bookmarks {
            match bk {
                Ok(b) => {
                    found_any = true;
                    println!("- {} ({})", bold(&b.name), b.url);
                }
                Err(e) => {
                    println!("{}: error reading bookmark: {}", red("Error"), e);
                }
            }
        }

        if !found_any {
            println!("No bookmarks found.");
        }
        Ok(())
    }
}

// 2
impl Command for BookmarkCommand {
    fn get_name(&self) -> &str {
        "bk"
    }

    fn exec(&mut self, args: &[&str]) {
        if args.is_empty() {
            println!("{}: 'bk' needs 'add' or 'search'", red("Error"));
            return;
        }

        let sub_cmd = args[0];
        match sub_cmd {
            "add" => {
                if args.len() != 3 {
                    println!("{}: 'bk add' needs <name> <url>", red("Error"));
                    return;
                }
                match self.add(args[1], args[2]) {
                    Ok(_) => {
                        println!("{} Added bookmark '{}'", green("Success:"), args[1]);
                    }
                    Err(e) => {
                        println!("{}: Failed to add: {}", red("Error"), e);
                    }
                }
            }
            "search" => {
                if args.len() != 2 {
                    println!("{}: 'bk search' needs <name>", red("Error"));
                    return;
                }
                match self.search(args[1]) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("{}: Failed to search: {}", red("Error"), e);
                    }
                }
            }
            _ => {
                println!("{}: unknown 'bk' command '{}'", red("Error"), sub_cmd);
            }
        }
    }
}


struct Terminal {
    commands: HashMap<String, Box<dyn Command>>,
}

impl Terminal {
    fn new() -> Terminal {
        Terminal {
            commands: HashMap::new(),
        }
    }

    fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.insert(cmd.get_name().to_string(), cmd);
    }

    fn run(&mut self) {
        let filename = "file";

        let file = match File::open(filename) {
            Ok(f) => f,
            Err(e) => {
                println!("{}: Could not open file '{}': {}", red("Error"), filename, e);
                return;
            }
        };

        let reader = BufReader::new(file);
        for line_result in reader.lines() {
            
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    println!("{}: Error reading line: {}", red("Error"), e);
                    continue; 
                }
            };
            
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            
            if parts.is_empty() {
                continue;
            }

            let cmd_name = parts[0];
            let args = &parts[1..]; 

            if cmd_name.eq_ignore_ascii_case("stop") {
                println!("{}", blue("Stopping terminal."));
                break; 
            }

            match self.commands.get_mut(cmd_name) {
                Some(cmd) => {
                    cmd.exec(args);
                }
                None => {
                    let lower_cmd = cmd_name.to_lowercase();
                    if self.commands.contains_key(&lower_cmd) {
                        println!(
                            "{}: Unknown command '{}'. Did you mean '{}'?",
                            red("Error"),
                            cmd_name,
                            green(&lower_cmd)
                        );
                    } else {
                        println!("{}: Unknown command '{}'.", red("Error"), cmd_name);
                    }
                }
            }
        }
    }
}


fn create_sample_file() {
    let content = b"
ping
ping abc

count
count a b c
count a
count a             b
   count a             b   

times
times
times abc

stop

bk add foxes https://www.reddit.com/r/foxes/
bk add rust_book https://doc.rust-lang.org/book/
bk add foxes_pics https://www.boredpanda.com/beautiful-fox-pictures/

bk search fox
bk search rust
";
    match File::create("file") {
        Ok(mut f) => {
            let _ = f.write_all(content);
        }
        Err(_) => {
            println!("{}: Could not create sample", red("Warning"));
        }
    }
}

fn main() {
    create_sample_file();

    println!("{}", bold(&blue("\nEx.1")));
    let mut terminal = Terminal::new();

    terminal.register(Box::new(PingCommand {}));
    terminal.register(Box::new(CountCommand {}));
    terminal.register(Box::new(TimesCommand { count: 0 }));
    
    terminal.register(Box::new(EchoCommand {}));

    println!("{}", bold(&blue("\nEx.2")));
    match BookmarkCommand::new("bookmarks.db") {
        Ok(bk_cmd) => {
            terminal.register(Box::new(bk_cmd));
        }
        Err(e) => {
            println!("{}: Could not load 'bk' command: {}", red("Error"), e);
        }
    }

    terminal.run();
}