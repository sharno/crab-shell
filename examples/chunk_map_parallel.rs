use crab_shell::prelude::*;

fn main() -> crab_shell::Result<()> {
    #[cfg(feature = "parallel")]
    {
        let results: Vec<_> = Shell::from_iter(0..10)
            .chunk_map_parallel(3, |chunk| {
                chunk.into_iter().map(|n| n * n).collect()
            })
            .collect();
        println!("Squares via chunk_map_parallel: {results:?}");
    }
    #[cfg(not(feature = "parallel"))]
    {
        println!("Enable the `parallel` feature to run this example.");
    }
    Ok(())
}
