use clap::Parser;

#[derive(Parser)]
#[command(name = "tb-bug", version, about = "Bugsnag insights CLI")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("tb-bug v{}", tb_bug::VERSION);
}
