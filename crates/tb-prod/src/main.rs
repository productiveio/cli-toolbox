use clap::Parser;

#[derive(Parser)]
#[command(name = "tb-prod", version, about = "Productive.io API CLI")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("tb-prod v{}", tb_prod::VERSION);
}
