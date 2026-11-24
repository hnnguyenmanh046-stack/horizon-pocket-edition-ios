//! What this version of the game writes for a shared world, and whether it reads what another
//! version wrote (.github/workflows/shared-worlds.yml): the same file is built at an older
//! release's tag, so each of the two reads the other's records.
//!
//!     format_probe write FOLDER    # FOLDER/manifest.json and FOLDER/lease.json, as saved now
//!     format_probe read FOLDER     # one line each: "manifest: read" or "manifest: refused: why"
//!
//! A world a newer version hosted or saved has to be refused whole ("update to play it"): an
//! older game that opened it would write it back in its own format.
use horizon_shared_worlds::{
    lease::{Claim, Leases, Member},
    manifest::{GenId, Manifest, VERSION},
    store::{Condition, MemoryStore, Store},
};
use std::sync::Arc;

const KEY: &str = "w/probe/lease";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(command), Some(folder)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: format_probe write|read FOLDER");
        std::process::exit(2);
    };
    let folder = std::path::Path::new(folder);
    match command.as_str() {
        "write" => {
            std::fs::create_dir_all(folder).expect("the folder");
            let manifest = Manifest {
                version: VERSION,
                id: GenId {
                    epoch: 1,
                    seq: 1,
                    day: 20261124,
                },
                parent: None,
                by: "probe".into(),
                packs: Vec::new(),
                files: Vec::new(),
            };
            let bytes = manifest.to_bytes().expect("a manifest");
            std::fs::write(folder.join("manifest.json"), bytes).expect("manifest.json");
            // The lease as a host's claim writes it.
            let store = Arc::new(MemoryStore::new());
            let me = Member {
                uuid: format!("{:032x}", 1),
                name: "Probe".into(),
                reachable: false,
            };
            Leases::new(store.clone(), KEY)
                .claim(None, &me, "probe", Claim::Free)
                .expect("a free world's lease");
            let lease = store.get(KEY).expect("the store").expect("the lease").data;
            std::fs::write(folder.join("lease.json"), lease).expect("lease.json");
            println!("wrote format {VERSION}");
        }
        "read" => {
            let manifest = std::fs::read(folder.join("manifest.json")).expect("manifest.json");
            match Manifest::from_bytes(&manifest) {
                Ok(_) => println!("manifest: read"),
                Err(e) => println!("manifest: refused: {e}"),
            }
            let lease = std::fs::read(folder.join("lease.json")).expect("lease.json");
            let store = Arc::new(MemoryStore::new());
            store
                .put(KEY, &lease, &Condition::Always)
                .expect("the store");
            match Leases::new(store, KEY).read() {
                Ok(Some(_)) => println!("lease: read"),
                Ok(None) => println!("lease: none"),
                Err(e) => println!("lease: refused: {e}"),
            }
        }
        _ => {
            eprintln!("usage: format_probe write|read FOLDER");
            std::process::exit(2);
        }
    }
}
