extern crate notify;

use notify::{watcher, RecursiveMode, Watcher};
use std::fs::*;
use std::io;
use std::path;
use std::path::Path;
use std::process::*;
use std::sync::mpsc::channel;
use std::time::Duration;

fn main() {
    // println!("Hello, world!");
    // let backup_base_path = Path::new("/home/navyd/tmp/.backup");
    // let hold_duration = Duration::from_secs(60);
    // let backup_duration = Duration::from_secs(60);
    // let config = Configuration::new(backup_base_path, hold_duration, backup_duration);

    // let paths = vec!["/home/navyd/tmp/test"];
    // watch(&paths, config);
    // watch()
    let mut from_paths = HashMap::new();
    from_paths.insert(
        Path::new("/home/navyd/tmp/test").to_path_buf(),
        RecursiveMode::Recursive,
    );
    let config = Configuration {
        commit_duration: Duration::from_secs(5),
        from_paths,
        name: "test".to_string(),
    };
    let backup_base_path = Path::new("/home/navyd/tmp/.backup");
    let context = BackupContext::new(vec![config], backup_base_path);
    let server = BackupServer::new(context);
    server.start();
    let context = server.get_context();
    loop {
        thread::sleep(Duration::from_secs(2));
        let a = context.holding_paths.lock().unwrap();
        println!("{:?}", a)
    }
}

pub struct BackupServer {
    backup_context: Arc<BackupContext>,
}

impl BackupServer {
    pub fn new(context: BackupContext) -> Self {
        BackupServer {
            backup_context: Arc::new(context),
        }
    }

    pub fn get_context(&self) -> &BackupContext {
        &self.backup_context
    }

    pub fn start(&self) {
        let context = Arc::clone(&self.backup_context);
        let config = Arc::clone(&self.backup_context.configurations);
        thread::spawn(move || {
            let (tx, rx) = channel();
            let mut watcher = watcher(tx, Duration::from_secs(3)).expect("watcher start failed");
            for config in config.iter() {
                for (path, mode) in &config.from_paths {
                    if let Err(e) = watcher.watch(path, *mode) {
                        eprintln!("{} watch error: {}", path.to_str().unwrap(), e);
                    }
                }
            }
            loop {
                match rx.recv() {
                    Ok(e) => {
                        match e {
                            notify::DebouncedEvent::Create(b)
                            | notify::DebouncedEvent::Write(b)
                            | notify::DebouncedEvent::Remove(b) => {
                                if let Err(e) = context.hold(b.as_path()) {
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
        });        
    }
}

pub struct Configuration {
    from_paths: HashMap<PathBuf, RecursiveMode>,
    commit_duration: Duration,
    name: String,
}

impl Configuration {}

use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub struct BackupContext {
    configurations: Arc<Vec<Configuration>>,
    holding_paths: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    backup_base_path: PathBuf,
}

impl BackupContext {
    pub fn new(configurations: Vec<Configuration>, backup_base_path: &Path) -> Self {
        BackupContext {
            configurations: Arc::new(configurations),
            backup_base_path: backup_base_path.to_path_buf(),
            holding_paths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn commit(&self, path: &Path) -> io::Result<()> {
        Command::new("git")
            .arg("add")
            .arg(path)
            .current_dir(self.backup_base_path.as_path())
            .stdout(Stdio::inherit())
            .spawn()
            .and_then(|mut child| child.wait())?;
        Command::new("git")
            .arg("commit")
            .arg(path)
            .args(("-m 'commit_'").split(" "))
            .current_dir(self.backup_base_path.as_path())
            .stdout(Stdio::inherit())
            .spawn()
            .and_then(|mut child| child.wait())?;
        Ok(())
    }

    /// 尝试将from_path的文件复制保存
    ///
    /// 如果上次保存时间不超过self.hold_duration则会返回ErrorKind::Other
    ///
    /// 如果from path是一个path则会返回一个ErrorKind::InvalidInput，暂不支持目录
    pub fn hold(&self, from_path: &Path) -> std::io::Result<()> {
        // 不支持目录
        if !from_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported for path:".to_owned() + from_path.to_str().unwrap(),
            ));
        }
        let backup_path = self.get_backup_file_path(from_path)?;
        let contents = read_to_string(from_path).unwrap();
        write(backup_path, contents)?;
        let mut h = self.holding_paths.lock().unwrap();
        h.insert(from_path.to_path_buf(), Instant::now());
        Ok(())
    }

    fn get_backup_file_path(&self, from_path: &Path) -> io::Result<path::PathBuf> {
        // path.join()对绝对路径将替换 base_path+from+path
        let mut path_str = String::new();
        path_str.push_str(self.backup_base_path.to_str().unwrap());
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
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

#[allow(unused)]
enum PackageManager {
    AptGet,
    Pacman,
    Other,
}

#[allow(unused)]
impl PackageManager {
    pub fn install(&self, name: &str) -> io::Result<()> {
        match self {
            AptGet => Command::new("apt-get")
                .arg("install")
                .arg(name)
                .spawn()?
                .wait()
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("未知错误: code={}", status.code().unwrap()),
                        ))
                    }
                }),
            _ => panic!("unsupported!"),
        }
    }

    pub fn install_multiple(&self, names: Vec<&str>) -> io::Result<()> {
        match self {
            AptGet => Command::new("apt-get")
                .arg("install")
                .args(names)
                .spawn()?
                .wait()
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("未知错误: code={}", status.code().unwrap()),
                        ))
                    }
                }),
            _ => panic!("unsupported!"),
        }
    }
}
