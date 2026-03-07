use clap::Parser;

#[derive(Parser)]
#[command(name = "tb-sem", version, about = "Semaphore CI insights CLI")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("tb-sem v{}", tb_sem::VERSION);
}
