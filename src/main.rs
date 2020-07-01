extern crate notify;

use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::path::Path;
use std::process::*;
use std::fs::*;

fn main() -> std::io::Result<()> {
    println!("Hello, world!");

    let backup_base_path = Path::new("/home/navyd/tmp/.backup");
    if !backup_base_path.exists() {
        println!("making dir: {}", backup_base_path.to_str().unwrap());
        create_dir(backup_base_path)?;
    } else if backup_base_path.is_file() {
        panic!("{} is not dir ", backup_base_path.canonicalize().unwrap().to_str().unwrap());
    }
    
    let paths = vec!["/etc/mysql/mysql.conf.d", "/home/navyd/tmp/a.txt"];
    watch(&paths, backup_base_path);
    // watch()
    Ok(())
}

fn watch(paths: &Vec<&str>, base_path: &Path) {
    
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_secs(3)).unwrap();
    for path in paths {
        let path = Path::new(path);
        watcher.watch(path, RecursiveMode::Recursive).unwrap();
    }
    loop {
        match rx.recv() {
            Ok(e) => {
                print!("event: {:?} ", e);
                match e {
                    notify::DebouncedEvent::Create(b) 
                    | notify::DebouncedEvent::Write(b) => {
                        let backup_path = base_path.join(b.as_path());
                        println!("backup path: {}", backup_path.to_str().unwrap());
                        if let Err(e) = backup(backup_path.as_path()) {
                            println!("backup error: {}", e);
                        }
                    },
                    notify::DebouncedEvent::Remove(b) => {
                        println!("removed {}", b.to_str().unwrap());
                    },
                    _ => {},
                };
            },
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}

fn backup(path: &Path) -> std::io::Result<()> {
    let contents = read_to_string(path).unwrap();
    write(path, contents)?;
    Command::new("git")
        .args("add .".split(" "))
        .stdout(Stdio::inherit())
        .spawn()
        .and_then(|mut child| child.wait())?;
    Command::new("git")
        .args("commit -m 'commit_'".split(" "))
        .stdout(Stdio::inherit())
        .spawn()
        .and_then(|mut child| child.wait())?;
    Ok(())
}
