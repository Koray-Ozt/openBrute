use std::path::Path;
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Copy)]
pub enum WordlistMode {
    /// Try every password for every username (NxM combinations)
    Cartesian,
    /// Try corresponding lines of username and password files (1-to-1)
    OneToOne,
}

pub struct CredentialSource {
    usernames: Vec<String>,
    passwords: Vec<String>,
    mode: WordlistMode,
    current_u: usize,
    current_p: usize,
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
            current_u: 0,
            current_p: 0,
        })
    }

    pub fn from_lists(usernames: Vec<String>, passwords: Vec<String>, mode: WordlistMode) -> Self {
        Self {
            usernames,
            passwords,
            mode,
            current_u: 0,
            current_p: 0,
        }
    }

    pub fn total_attempts(&self) -> usize {
        match self.mode {
            WordlistMode::Cartesian => self.usernames.len() * self.passwords.len(),
            WordlistMode::OneToOne => self.usernames.len().min(self.passwords.len()),
        }
    }
}

impl Iterator for CredentialSource {
    type Item = Credentials;

    fn next(&mut self) -> Option<Self::Item> {
        if self.usernames.is_empty() || self.passwords.is_empty() {
            return None;
        }

        match self.mode {
            WordlistMode::Cartesian => {
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
