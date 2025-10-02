use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("modsync-cli: called with args: {:?}", args);
    println!("This is a placeholder CLI. No actions implemented yet.");
}
