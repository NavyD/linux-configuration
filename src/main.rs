extern crate notify;

use notify::{watcher, RecursiveMode, Watcher};
use std::fs::*;
use std::io;
use std::path;
use std::path::Path;
use std::process::*;
use std::sync::mpsc::channel;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    let backup_base_path = Path::new("/home/navyd/tmp/.backup");
    let hold_duration = Duration::from_secs(60);
    let backup_duration = Duration::from_secs(60);
    let config = Configuration::new(backup_base_path, hold_duration, backup_duration);

    let paths = vec!["/home/navyd/tmp/test"];
    watch(&paths, config);
    // watch()
    Ok(())
}

fn watch(paths: &Vec<&str>, mut config: Configuration) {
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_secs(3)).unwrap();
    for path in paths {
        let path = Path::new(path);
        watcher.watch(path, RecursiveMode::Recursive).unwrap();
    }
    
    loop {
        match rx.recv() {
            Ok(e) => {
                match e {
                    notify::DebouncedEvent::Create(b)
                    | notify::DebouncedEvent::Write(b)
                    | notify::DebouncedEvent::Remove(b) => {
                        if let Err(e) = config.hold_backup(b.as_path()) {
                            eprintln!("config hold error: {}", e);
                        } else {
                            println!("{} 已复制", b.to_str().unwrap());
                        }
                    }
                    _ => {}
                };
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

pub struct Configuration<'a> {
    path_instants: HashMap<PathBuf, Instant>,
    backup_duration: Duration,
    hold_duration: Duration,
    base_path: &'a Path,
}

impl<'a> Configuration<'a> {
    pub fn new(base_path: &'a Path, hold_duration: Duration, backup_duration: Duration) -> Self {
        if !base_path.exists() {
            create_dir(base_path).unwrap();
        } else if base_path.is_file() {
            panic!(
                "{} is not dir ",
                base_path.canonicalize().unwrap().to_str().unwrap()
            );
        }
    
        Configuration {
            path_instants: HashMap::new(),
            hold_duration,
            backup_duration,
            base_path,
        }
    }

    /// 尝试将from_path的文件复制保存
    /// 
    /// 如果上次保存时间不超过self.hold_duration则会返回ErrorKind::Other
    /// 
    /// 如果from path是一个path则会返回一个ErrorKind::InvalidInput，暂不支持目录
    pub fn hold_backup(&mut self, from_path: &Path) -> std::io::Result<()> {
        // 不支持目录
        if !from_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported for path:".to_owned() + from_path.to_str().unwrap(),
            ));
        }
        if !self.is_hold(from_path) { 
            return Err(io::Error::new(io::ErrorKind::Other, "距上次hold duration太短"));
        }
        let base_path = self.base_path;
        // 先创建backup目录
        let get_backup_file_path = || -> io::Result<path::PathBuf> {
            // path.join()对绝对路径将替换 base_path+from+path
            let mut path_str = String::new();
            path_str.push_str(base_path.to_str().unwrap());
            let mut p = from_path.to_path_buf();
            // create dir可能将a.txt 作为目录创建 先移除
            let filename = p.file_name().unwrap().to_str().unwrap().to_owned();
            p.pop();
            path_str.push_str(p.to_str().unwrap());
            let mut backup_path = path::PathBuf::from(path_str);
            if !backup_path.exists() {
                create_dir_all(backup_path.as_path())?;
            }
            backup_path.push(filename);
            Ok(backup_path)
        };

        let backup_path = get_backup_file_path()?;
        let contents = read_to_string(from_path).unwrap();
        write(backup_path, contents)?;
        self.path_instants.insert(from_path.to_path_buf(), Instant::now());
        // self.git_commit(base_path)
        Ok(())
    }

    fn is_hold(&self, from_path: &Path) -> bool {
        self.path_instants
            .get(from_path)
            .map(|start| start.elapsed() >= self.hold_duration)
            .unwrap_or(true)
    }

    fn git_commit(&self, cur_dir: &Path) -> io::Result<()> {
        Command::new("git")
            .args("add .".split(" "))
            .current_dir(cur_dir)
            .stdout(Stdio::inherit())
            .spawn()
            .and_then(|mut child| child.wait())?;
        Command::new("git")
            .args("commit -m 'commit_'".split(" "))
            .current_dir(cur_dir)
            .stdout(Stdio::inherit())
            .spawn()
            .and_then(|mut child| child.wait())?;
        Ok(())
    }
}
