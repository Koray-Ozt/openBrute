use std::path::Path;
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordlistMode {
    /// Try every password for every username (NxM combinations)
    Cartesian,
    /// Try corresponding lines of username and password files (1-to-1)
    OneToOne,
    /// Try credentials loaded directly from combo files (username:password)
    Combo,
}

pub struct CredentialSource {
    usernames: Vec<String>,
    passwords: Vec<String>,
    mode: WordlistMode,
    combos: Option<Vec<Credentials>>,
    current_u: usize,
    current_p: usize,
    current_c: usize,
}

impl CredentialSource {
    pub async fn new(
        username_path: &Path,
        password_path: &Path,
        mode: WordlistMode,
    ) -> Result<Self, io::Error> {
        let usernames = read_lines(username_path).await?;
        let passwords = read_lines(password_path).await?;
        Ok(Self {
            usernames,
            passwords,
            mode,
            combos: None,
            current_u: 0,
            current_p: 0,
            current_c: 0,
        })
    }

    pub fn from_lists(usernames: Vec<String>, passwords: Vec<String>, mode: WordlistMode) -> Self {
        Self {
            usernames,
            passwords,
            mode,
            combos: None,
            current_u: 0,
            current_p: 0,
            current_c: 0,
        }
    }

    pub async fn new_combo(path: &Path) -> Result<Self, io::Error> {
        let combos = read_combo_lines(path).await?;
        Ok(Self {
            usernames: Vec::new(),
            passwords: Vec::new(),
            mode: WordlistMode::Combo,
            combos: Some(combos),
            current_u: 0,
            current_p: 0,
            current_c: 0,
        })
    }

    pub fn from_combos(combos: Vec<Credentials>) -> Self {
        Self {
            usernames: Vec::new(),
            passwords: Vec::new(),
            mode: WordlistMode::Combo,
            combos: Some(combos),
            current_u: 0,
            current_p: 0,
            current_c: 0,
        }
    }

    pub fn total_attempts(&self) -> usize {
        match self.mode {
            WordlistMode::Cartesian => self.usernames.len() * self.passwords.len(),
            WordlistMode::OneToOne => self.usernames.len().min(self.passwords.len()),
            WordlistMode::Combo => self.combos.as_ref().map(|c| c.len()).unwrap_or(0),
        }
    }
}

impl Iterator for CredentialSource {
    type Item = Credentials;

    fn next(&mut self) -> Option<Self::Item> {
        match self.mode {
            WordlistMode::Combo => {
                let combos = self.combos.as_ref()?;
                if self.current_c >= combos.len() {
                    None
                } else {
                    let creds = combos[self.current_c].clone();
                    self.current_c += 1;
                    Some(creds)
                }
            }
            WordlistMode::Cartesian => {
                if self.usernames.is_empty() || self.passwords.is_empty() {
                    return None;
                }
                if self.current_u >= self.usernames.len() {
                    return None;
                }
                let creds = Credentials {
                    username: self.usernames[self.current_u].clone(),
                    password: self.passwords[self.current_p].clone(),
                };
                self.current_p += 1;
                if self.current_p >= self.passwords.len() {
                    self.current_p = 0;
                    self.current_u += 1;
                }
                Some(creds)
            }
            WordlistMode::OneToOne => {
                if self.usernames.is_empty() || self.passwords.is_empty() {
                    return None;
                }
                let idx = self.current_u;
                if idx >= self.usernames.len() || idx >= self.passwords.len() {
                    return None;
                }
                let creds = Credentials {
                    username: self.usernames[idx].clone(),
                    password: self.passwords[idx].clone(),
                };
                self.current_u += 1;
                Some(creds)
            }
        }
    }
}

async fn read_lines(path: &Path) -> Result<Vec<String>, io::Error> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut result = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            result.push(trimmed.to_string());
        }
    }
    Ok(result)
}

async fn read_combo_lines(path: &Path) -> Result<Vec<Credentials>, io::Error> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut result = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(pos) = trimmed.find(':') {
            let username = trimmed[..pos].trim().to_string();
            let password = trimmed[pos + 1..].trim().to_string();
            result.push(Credentials { username, password });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_combo_file_parsing() {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join("test_combos.txt");
        let content = "admin:admin123\nuser1:secret\n# comment line\n\n  guest  :  pass  \n";
        tokio::fs::write(&temp_file_path, content).await.unwrap();

        let source = CredentialSource::new_combo(&temp_file_path).await.unwrap();
        assert_eq!(source.total_attempts(), 3);

        let mut iter = source;
        assert_eq!(iter.next(), Some(Credentials { username: "admin".to_string(), password: "admin123".to_string() }));
        assert_eq!(iter.next(), Some(Credentials { username: "user1".to_string(), password: "secret".to_string() }));
        assert_eq!(iter.next(), Some(Credentials { username: "guest".to_string(), password: "pass".to_string() }));
        assert_eq!(iter.next(), None);

        let _ = tokio::fs::remove_file(&temp_file_path).await;
    }
}
