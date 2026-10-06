//! SMB3 storage: encrypted authenticated sessions, no DFS redirects or ambient credentials.
use crate::{validate_relative_path, StorageConnection};
use anyhow::{bail, ensure, Result};
use smb::{
    Client, ClientConfig, CreateOptions, File, FileAccessMask, FileAttributes, FileCreateArgs,
    GetLen, ReadAt, Resource, SetLen, UncPath, WriteAt,
};
use std::{str::FromStr, time::Duration};

pub struct SmbStorageOptions<'a> {
    pub host: String,
    pub port: u16,
    pub share: String,
    pub root: String,
    pub username: String,
    pub password: &'a str,
}
pub fn connect(options: SmbStorageOptions<'_>) -> Result<Box<dyn StorageConnection>> {
    ensure!(
        !options.host.is_empty()
            && !options.host.contains(['/', '\\'])
            && !options.share.is_empty()
            && !options.share.contains(['/', '\\']),
        "invalid SMB endpoint"
    );
    ensure!(
        options.root.starts_with('/'),
        "absolute SMB share root required"
    );
    let root = options
        .root
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_owned();
    if !root.is_empty() {
        validate_relative_path(&root)?;
    }
    let mut config = ClientConfig::default();
    config.dfs = false;
    config.connection.port = Some(options.port);
    config.connection.timeout = Some(Duration::from_secs(10));
    config.connection.encryption_mode = smb::connection::config::EncryptionMode::Required;
    config.connection.allow_unsigned_guest_access = false;
    config.connection.compression_enabled = false;
    let client = Client::new(config);
    let share = UncPath::from_str(&format!("\\\\{}\\{}", options.host, options.share))?;
    if let Err(error) = client.share_connect(&share, &options.username, options.password.to_owned())
    {
        let _ = client.close();
        return Err(error.into());
    }
    Ok(Box::new(SmbStorage {
        client,
        share,
        root,
    }))
}
struct SmbStorage {
    client: Client,
    share: UncPath,
    root: String,
}
impl SmbStorage {
    fn path(&self, relative: &str) -> Result<String> {
        if relative.starts_with(".ctox-transfer-") {
            ensure!(
                relative.len() == 84
                    && relative.ends_with(".part")
                    && relative[15..79].bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid storage staging path"
            );
        } else {
            validate_relative_path(relative)?;
        }
        Ok(if self.root.is_empty() {
            relative.into()
        } else {
            format!("{}/{relative}", self.root)
        }
        .replace('/', "\\"))
    }
    fn open(&self, path: &str, write: bool, create: bool) -> Result<File> {
        let path = self.path(path)?;
        // Open every parent as a reparse point and reject it before traversing.
        let pieces: Vec<_> = path.split('\\').collect();
        for end in 1..pieces.len() {
            let mut args =
                FileCreateArgs::make_open_existing(FileAccessMask::new().with_generic_read(true));
            args.options = args.options.with_open_reparse_point(true);
            let resource = self
                .client
                .create_file(&self.share.with_path(pieces[..end].join("\\")), &args)?;
            let Resource::Directory(directory) = resource else {
                bail!("SMB parent is not a directory");
            };
            let info = directory.query_info::<smb::FileAttributeTagInformation>()?;
            let allowed = !info.file_attributes.reparse_point();
            directory.close()?;
            ensure!(allowed, "SMB reparse point is forbidden");
        }
        let access = FileAccessMask::new()
            .with_generic_read(true)
            .with_generic_write(write)
            .with_delete(write);
        let mut args = if create {
            FileCreateArgs::make_create_new(FileAttributes::new(), CreateOptions::new())
        } else {
            FileCreateArgs::make_open_existing(access)
        };
        args.desired_access = access;
        args.options = args
            .options
            .with_open_reparse_point(true)
            .with_non_directory_file(true);
        let resource = self
            .client
            .create_file(&self.share.with_path(path), &args)?;
        let Resource::File(file) = resource else {
            bail!("SMB resource is not a file");
        };
        let info = file.query_info::<smb::FileAttributeTagInformation>()?;
        ensure!(
            !info.file_attributes.reparse_point(),
            "SMB reparse point is forbidden"
        );
        Ok(file)
    }
}
impl StorageConnection for SmbStorage {
    fn length(&mut self, path: &str) -> Result<Option<u64>> {
        match self.open(path, false, false) {
            Ok(file) => {
                let length = file.get_len()?;
                file.close()?;
                Ok(Some(length))
            }
            Err(error) => {
                if matches!(
                    error.downcast_ref::<smb::Error>(),
                    Some(smb::Error::ReceivedErrorMessage(0xc0000034 | 0xc000003a, _))
                ) {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }
    fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        ensure!(length <= 1024 * 1024, "storage range too large");
        let file = self.open(path, false, false)?;
        let mut bytes = vec![0; length];
        let mut done = 0;
        while done < length {
            let n = file.read_at(&mut bytes[done..], offset + done as u64)?;
            ensure!(n > 0, "incomplete SMB read");
            done += n;
        }
        file.close()?;
        Ok(bytes)
    }
    fn create(&mut self, path: &str) -> Result<()> {
        let file = self.open(path, true, true)?;
        file.flush()?;
        file.close()?;
        Ok(())
    }
    fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        let file = self.open(path, true, false)?;
        file.set_len(length)?;
        file.flush()?;
        file.close()?;
        Ok(())
    }
    fn write(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        let file = self.open(path, true, false)?;
        let mut done = 0;
        while done < bytes.len() {
            let n = file.write_at(&bytes[done..], offset + done as u64)?;
            ensure!(n > 0, "incomplete SMB write");
            done += n;
        }
        file.flush()?;
        file.close()?;
        Ok(())
    }
    fn publish(&mut self, staging: &str, destination: &str) -> Result<()> {
        let file = self.open(staging, true, false)?;
        file.set_info(smb::FileRenameInformation {
            replace_if_exists: false.into(),
            root_directory: 0,
            file_name: self.path(destination)?.into(),
        })?;
        file.flush()?;
        file.close()?;
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.client.close()?;
        Ok(())
    }
}
