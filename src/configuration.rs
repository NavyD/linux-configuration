use notify::{watcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use std::cell::RefCell;
use std::env;
use std::io;
use std::process::*;

extern crate regex;

use regex::Regex;

#[allow(unused)]
pub enum PackageManager {
    AptGet,
    Pacman,
    Other,
}

#[allow(unused)]
impl PackageManager {
    pub fn install(&self, name: &str) -> io::Result<()> {
        match self {
            AptGet => Command::new("sudo")
                .arg("apt-get")
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

    pub fn uninstall(&self, name: &str) -> io::Result<()> {
        Ok(())
    }
}

fn exec(command: &str) -> Result<String, String> {
    let comm: Vec<&str> = command.split(' ').collect();
    let out = Command::new(comm.get(0).expect(command))
        .args(comm.get(1..).expect(command))
        .output()
        .expect(command);
    if out.status.success() {
        Ok(std::str::from_utf8(&out.stdout).unwrap().to_string())
    } else {
        Err(std::str::from_utf8(&out.stderr).unwrap().to_string())
    }
}

fn exec_in_dir(command: &str, dir: &str) -> Result<String, String> {
    let comm: Vec<&str> = command.split(' ').collect();
    let out = Command::new(comm.get(0).expect(command))
        .args(comm.get(1..).expect(command))
        .current_dir(Path::new(dir))
        .output()
        .expect(command);
    if out.status.success() {
        Ok(std::str::from_utf8(&out.stdout).unwrap().to_string())
    } else {
        Err(std::str::from_utf8(&out.stderr).unwrap().to_string())
    }
}

/// -----------

pub trait Program {
    fn get_name(&self) -> &str;

    fn get_package_manager(&self) -> &PackageManager;

    fn install(&self) -> io::Result<()> {
        if self.exists() {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} 已安装", self.get_name()),
            ))
        } else {
            self.get_package_manager().install(self.get_name())
        }
    }

    fn uninstall(&self) -> io::Result<()> {
        self.get_package_manager().uninstall(self.get_name())
    }

    fn config(&self) -> io::Result<()>;

    /// 通过sh command -v $name验证
    fn exists(&self) -> bool {
        Command::new("sh")
            .arg("-c")
            .arg("command")
            .arg("-v")
            .arg(self.get_name())
            .spawn()
            .and_then(|mut child| child.wait())
            .map(|status| status.success())
            .unwrap_or(false)
    }
}
#[allow(unused)]
pub struct ZshProgram<'a> {
    zshrc_content: &'a ShellConfiguration,
    zsh_home: String,
}
#[allow(unused)]
impl<'a> ZshProgram<'a> {
    pub fn new(content: &'a ShellConfiguration, zsh_home: &str) -> Self {
        ZshProgram {
            zshrc_content: content,
            zsh_home: zsh_home.to_string(),
        }
    }

    pub fn config(&mut self) {}

    fn config_plugins(&mut self) {
        let mut plugins = self
            .zshrc_content
            .get_var("plugins")
            .expect("从zsh获取plugins变量失败");
        plugins
            .trim()
            .strip_suffix(")")
            .expect("trim后的plugins不是以)结尾");
        let mut plugins = plugins.to_string();

        self.config_plugin_with_git(
            &mut plugins,
            "zsh-autosuggestions",
            "https://github.com/zsh-users/zsh-autosuggestions",
        );
        self.config_plugin_with_git(
            &mut plugins,
            "zsh-syntax-highlighting",
            "https://github.com/zsh-users/zsh-syntax-highlighting.git",
        );
    }

    fn config_plugin_with_git(&self, plugins: &mut String, name: &str, url: &str) {
        match self.get_plugin_with_git_clone(name, url) {
            Ok(_) => plugins.push_str(&name),
            Err(e) => match e.kind() {
                io::ErrorKind::AlreadyExists => {
                    if !plugins.contains(name) {
                        plugins.push_str(name);
                    }
                }
                _ => {
                    eprintln!("{}", e);
                }
            },
        }
    }

    fn get_plugin_with_git_clone(&self, name: &str, url: &str) -> io::Result<String> {
        let path = self.zsh_home.to_owned() + "custom/plugins/" + name;
        let path = Path::new(&path);
        if path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("path: {} 已存在", path.to_str().unwrap()),
            ));
        }

        let mut command = String::from("git clone ");
        command.push_str(url);
        command.push_str(" ");
        command.push_str(path.to_str().unwrap());
        // download plugin zsh-autosuggestions
        if let Err(e) = exec(&command) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{} 下载错误：{}", command, e),
            ));
        }
        Ok(name.to_string())
    }
}

/// `(?m)`表示在Regex用[flags](https://docs.rs/regex/1.3.9/regex/index.html#grouping-and-flags) multi-line
const REG_VAR: &str = r"(?m)^(\s*)(\w*?)(\s*)(\w+)=(.+?)$";

pub struct ShellConfiguration {
    content: String,
    var_reg: Regex,
}

#[allow(unused)]
impl ShellConfiguration {
    pub fn new(content: &str) -> Self {
        ShellConfiguration {
            content: content.to_string(),
            var_reg: Regex::new(REG_VAR).unwrap(),
        }
    }

    /// export已存在的变量并返回该变量，如果不存在则返回None
    pub fn export_var(&mut self, name: &str) -> Option<String> {
        let reg = Regex::new(&(r"(?m)^(\s*)(\w*?)(\s*)".to_string() + name + r"=(.+?)$")).unwrap();
        for cap in reg.captures_iter(&self.content.clone()) {
            let exported = &cap[2];
            let val = &cap[4];
            if exported == "export" {
                return Some(val.to_string())
            } else if exported.is_empty() {
                // self.content = reg.replace(&self.content, ("$1export $3".to_string() + "test" + "=$4").as_str()).to_string();
                self.content = reg.replace(&self.content, ("${1}export ${3}".to_string() + name + "=${4}").as_str()).to_string();
                return Some(val.to_string());
            }
        }
        None
    }

    /// 如果`export name=val`则返回true，否则返回false
    pub fn is_exported_var(&self, name: &str) -> bool {
        let reg = Regex::new(&(r"(?m)^(\s*)(export)(\s*)".to_string() + name + r"=(.+?)$")).unwrap();
        reg.is_match(&self.content)
    }

    pub fn remove_var(&mut self, name: &str) -> Option<String> {
        let reg = Regex::new(&(r"(?m)^(\s*)(\w*?)(\s*)".to_string() + name + r"=(.+?)$")).unwrap();
        for cap in reg.captures_iter(&self.content.clone()) {
            let val = &cap[4];
            self.content = reg.replace(&self.content, "").to_string();
            return Some(val.to_string());
        }
        None
    }

    /// 创建一个name=val local变量并返加之前的old_val，如果不存在old_val则None
    /// 
    /// 如果val要用""包裹，必需主动在val中写出
    /// 
    /// ```no_run
    /// let val = "\"new_path/to\"";
    /// ```
    pub fn put_var(&mut self, name: &str, val: &str) -> Option<String> {
        if let Some(old_val) = self.get_var(name) {
            let r = self.var_reg
                .replace(&self.content, ("$1$2$3$4=".to_string() + val).as_str());
            self.content = r.to_string();
            Some(old_val)
        } else {
            self.content.push('\n');
            self.content.push_str(name);
            self.content.push('=');
            self.content.push_str(val);
            None
        }
    }

    /// 如果是存在""则直接返回如：`let val = "\"new_path/to\"";`，要手动处理
    /// 
    /// ```sh
    /// export ZSH="/home/navyd/.oh-my-zsh"
    /// ```
    pub fn get_var(&self, name: &str) -> Option<String> {
        for cap in self.var_reg.captures_iter(&self.content) {
            let key = &cap[4];
            let val = &cap[5];
            if key.trim() == name {
                return Some(val.to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod shell_configuration_tests {
    use super::*;

    #[test]
    fn reg_test() {
        let reg = Regex::new(REG_VAR).unwrap();
        assert!(reg.is_match(&get_shell_configuration().content));
    }

    #[test]
    fn get_with_prefix_export_or_others() {
        let shell = get_shell_configuration();
        // export ZSH=\"/home/navyd/.oh-my-zsh\"
        assert_eq!(shell.get_var("ZSH"), Some("\"/home/navyd/.oh-my-zsh\"".to_string()));
    }

    #[test]
    fn get_none_with_hash() {
        // 不允许#
        let shell = get_shell_configuration();
        assert_eq!(shell.get_var("ZSH_THEME_RANDOM_CANDIDATES"), None);
        assert_eq!(shell.get_var("HYPHEN_INSENSITIVE"), None);
    }

    #[test]
    fn basic_get() {
        let shell = get_shell_configuration();
        assert_eq!(shell.get_var("plugins"), Some("(git z extract zsh-autosuggestions zsh-syntax-highlighting docker docker-compose mvn)".to_string()))
    }

    #[test]
    fn put_new() {
        let mut shell = get_shell_configuration();
        let old_len = shell.content.len();
        let (name, val) = ("test_name", "test_val");
        assert_eq!(shell.get_var(name), None);
        assert!(shell.put_var(name, val).is_none());
        assert_eq!(shell.get_var(name), Some(val.to_string()));
        // 加了2个字符\n, =
        assert_eq!(shell.content.len(), old_len + (name.len() + 2 + val.len()));
    }

    #[test]
    fn put_old_with_quote() {
        let mut shell = get_shell_configuration();
        let old_len = shell.content.len();
        let (name, new_val) = ("ZSH", "\"/home/navyd/.oh-my-zsh/test\"");
        let old_val = "\"/home/navyd/.oh-my-zsh\"";
        assert_eq!(shell.get_var(name), Some(old_val.to_string()));
        assert_eq!(shell.put_var(name, new_val), Some(old_val.to_string()));
        assert_eq!(shell.get_var(name), Some(new_val.to_string()));
        // check 只替换val
        assert_eq!(shell.content.len(), old_len + (new_val.len() - old_val.len()));
    }

    #[test]
    fn remove_var_none() {
        let mut shell = get_shell_configuration();
        let none_name = "_none_name_test";
        assert!(shell.get_var(none_name).is_none());
        assert_eq!(shell.remove_var(none_name), None);
        assert_eq!(shell.get_var(none_name), None);
    }

    #[test]
    fn remove_old_var() {
        let mut shell = get_shell_configuration();
        let old_len = shell.content.len();
        let name = "ZSH";
        let old_val = shell.get_var(name);
        assert!(old_val.is_some());
        assert_eq!(shell.remove_var(name), old_val);
        assert_eq!(shell.get_var(name), None);
        // 被删size应该一致
        assert_eq!(shell.content.len(), old_len - "export ZSH=\"/home/navyd/.oh-my-zsh\"".len());
    }

    #[test]
    fn not_exported_var_in_none_name_and_local_name() {
        let shell = get_shell_configuration();
        let none_name = "_none_name_test";
        let local_name = "ZSH_THEME";
        assert!(!shell.is_exported_var(none_name));
        assert!(!shell.is_exported_var(local_name));
    }

    #[test]
    fn exported_var() {
        let shell = get_shell_configuration();
        let exported_name = "ZSH";
        assert!(shell.is_exported_var(exported_name));
    }

    #[test]
    fn export_var_failed_in_none_name() {
        let mut shell = get_shell_configuration();
        let none_name = "_none_name_test";
        assert!(shell.get_var(none_name).is_none());
        assert_eq!(shell.export_var(none_name), None);
    }

    #[test]
    fn export_var_with_local_var() {
        let mut shell = get_shell_configuration();
        let old_len = shell.content.len();
        let name = "ZSH_THEME";
        assert!(!shell.is_exported_var(name));
        assert_eq!(shell.export_var(name), Some("\"ys\"".to_string()));
        assert!(shell.get_var(name).is_some());
        assert!(shell.is_exported_var(name));
        assert!(old_len == (shell.content.len() - "export ".len()));
    }

    fn get_shell_configuration() -> ShellConfiguration {
        let content = "
# If you come from bash you might have to change your $PATH.
# export PATH=$HOME/bin:/usr/local/bin:$PATH

# Path to your oh-my-zsh installation.
export ZSH=\"/home/navyd/.oh-my-zsh\"

fpath+=~/.zfunc

# Set name of the theme to load --- if set to \"random\", it will
# load a random theme each time oh-my-zsh is loaded, in which case,
# to know which specific one was loaded, run: echo $RANDOM_THEME
# See https://github.com/ohmyzsh/ohmyzsh/wiki/Themes
ZSH_THEME=\"ys\"

#ZSH_THEME=\"agnoster\"

# Set list of themes to pick from when loading at random
# Setting this variable when ZSH_THEME=random will cause zsh to load
# a theme from this variable instead of looking in ~/.oh-my-zsh/themes/
# If set to an empty array, this variable will have no effect.
# ZSH_THEME_RANDOM_CANDIDATES=( \"robbyrussell\" \"agnoster\" )

# Uncomment the following line to use case-sensitive completion.
# CASE_SENSITIVE=\"true\"

# Uncomment the following line to use hyphen-insensitive completion.
# Case-sensitive completion must be off. _ and - will be interchangeable.
# HYPHEN_INSENSITIVE=\"true\"

# HIST_STAMPS=\"mm/dd/yyyy\"

# Would you like to use another custom folder than $ZSH/custom?
# ZSH_CUSTOM=/path/to/new-custom-folder

# Which plugins would you like to load?
# Standard plugins can be found in ~/.oh-my-zsh/plugins/*
# Custom plugins may be added to ~/.oh-my-zsh/custom/plugins/
# Example format: plugins=(rails git textmate ruby lighthouse)
# Add wisely, as too many plugins slow down shell startup.
plugins=(git z extract zsh-autosuggestions zsh-syntax-highlighting docker docker-compose mvn)

source $ZSH/oh-my-zsh.sh
";
        ShellConfiguration::new(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn basics() {
    //     let zsh = ZshProgram::new();
    //     if !zsh.exists() {
    //         zsh.install();
    //     }
    //     zsh.config();
    // }

    #[test]
    fn config_from_backup_file() {
        use std::env;

        // We will iterate through the references to the element returned by
        // env::vars();
        // let key = "HOME";
        // match env::var(key) {
        //     Ok(val) => println!("{}={}", key, val),
        //     Err(e) => println!("----{}", e),
        // }

        for (key, value) in env::vars() {
            println!("{}: {}", key, value);
        }
    }
}
