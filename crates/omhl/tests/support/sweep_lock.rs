use std::fs::{self, OpenOptions};

use nix::fcntl::{Flock, FlockArg};

/// `sweep_strays()` e `check run` varrem o `/proc` inteiro: em paralelo, um teste mataria o neto de
/// outro. O flock só protege contra outro `cargo` concorrente (os binários de um mesmo `cargo test`
/// já rodam em sequência).
pub fn serialize_sweeps() -> Flock<fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::env::temp_dir().join("omhl-process-sweep.lock"))
        .expect("the sweep lock file opens");
    Flock::lock(file, FlockArg::LockExclusive)
        .unwrap_or_else(|(_, errno)| panic!("the sweep lock is taken: {errno}"))
}
