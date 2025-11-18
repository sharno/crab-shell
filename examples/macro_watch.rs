use std::time::Duration;

use qshr::qshr;

fn main() -> qshr::Result<()> {
    qshr! {
        println!("Watching .rs files for the next five events...");
        let events = watch_filtered(".", Duration::from_millis(250), "**/*.rs")?;
        for event in events.take(5) {
            let event = event?;
            println!("changed -> {}", event.path().display());
        }
    }
}
