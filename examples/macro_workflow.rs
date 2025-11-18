use qshr::qshr;

fn main() -> qshr::Result<()> {
    let tracked = ["src/lib.rs", "src/shell.rs"];
    qshr! {
        println!("== formatting ==");
        "cargo fmt";
        env "RUST_BACKTRACE" = "1";
        "echo backtrace -> $RUST_BACKTRACE";

        println!("== running focused tests ==");
        "cargo test --lib";

        println!("== line counts for tracked files ==");
        for path in &tracked {
            let summary = cmd("wc").arg("-l").arg(path).read()?;
            print!("{summary}");
        };

        println!("== recent git status ==");
        {
            let status = cmd("git").args(["status", "--short"]).read()?;
            println!("{status}");
        };

        cd("src") {
            "ls";
        };

        parallel {
            "echo worker one";
        } {
            "echo worker two";
        };

        println!("== top TODO matches ==");
        "rg TODO -n src" | "head -n 5";
        unset "RUST_BACKTRACE";
    }?;
    Ok(())
}
