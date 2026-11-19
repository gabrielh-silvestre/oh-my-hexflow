use std::fs::{self, OpenOptions};

use nix::fcntl::{Flock, FlockArg};

/// `sweep_strays()` e `check sweep` varrem o `/proc` inteiro: em paralelo, um teste mataria o neto de
/// outro. O `check run` só varre a própria raiz, então não precisa desta trava. O flock só protege
/// contra outro `cargo` concorrente (os binários de um mesmo `cargo test` já rodam em sequência).
pub fn serialize_sweeps() -> Flock<fs::File> {
    lock_sweeps(FlockArg::LockExclusive)
}

/// Para quem só cria órfão de propósito: vários podem rodar juntos, mas nenhum varredor global
/// (`serialize_sweeps`) roda ao mesmo tempo. O teste só termina depois de os próprios órfãos morrerem.
pub fn share_sweeps() -> Flock<fs::File> {
    lock_sweeps(FlockArg::LockShared)
}

fn lock_sweeps(mode: FlockArg) -> Flock<fs::File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::env::temp_dir().join("omhl-process-sweep.lock"))
        .expect("the sweep lock file opens");
    Flock::lock(file, mode).unwrap_or_else(|(_, errno)| panic!("the sweep lock is taken: {errno}"))
}
