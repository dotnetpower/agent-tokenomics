use crate::model::CallRecord;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Mutex;

enum WriterBackend {
    Plain(BufWriter<File>),
    Gzip(GzEncoder<BufWriter<File>>),
}

pub struct JsonlWriter {
    pub path: String,
    inner: Mutex<Option<WriterBackend>>,
}

impl JsonlWriter {
    pub fn new(path: impl Into<String>) -> std::io::Result<Self> {
        let p = path.into();
        if let Some(parent) = Path::new(&p).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let is_gz = p.to_ascii_lowercase().ends_with(".gz");
        let mut opts = OpenOptions::new();
        opts.create(true).write(true);
        if is_gz {
            opts.truncate(true);
        } else {
            opts.append(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(&p)?;
        let buf = BufWriter::with_capacity(65_536, file);
        let backend = if is_gz {
            WriterBackend::Gzip(GzEncoder::new(buf, Compression::fast()))
        } else {
            WriterBackend::Plain(buf)
        };
        Ok(Self {
            path: p,
            inner: Mutex::new(Some(backend)),
        })
    }

    pub fn write_record(&self, rec: &CallRecord) {
        if let Ok(line) = serde_json::to_string(rec) {
            self.write_raw_line(&line);
        }
    }

    pub fn write_value(&self, val: &Value) {
        if let Ok(line) = serde_json::to_string(val) {
            self.write_raw_line(&line);
        }
    }

    pub fn write_raw_line(&self, line: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(ref mut backend) = *guard {
                match backend {
                    WriterBackend::Plain(w) => {
                        let _ = w.write_all(line.as_bytes());
                        let _ = w.write_all(b"\n");
                        let _ = w.flush();
                    }
                    WriterBackend::Gzip(w) => {
                        let _ = w.write_all(line.as_bytes());
                        let _ = w.write_all(b"\n");
                        let _ = w.flush();
                    }
                }
            }
        }
    }

    pub fn close(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(backend) = guard.take() {
                match backend {
                    WriterBackend::Plain(mut w) => {
                        let _ = w.flush();
                    }
                    WriterBackend::Gzip(w) => {
                        let _ = w.finish();
                    }
                }
            }
        }
    }
}

impl Drop for JsonlWriter {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug, Default, Clone)]
pub struct ReadStats {
    pub bad_lines: usize,
}

pub fn read_jsonl(path: &str, stats: Option<&mut ReadStats>) -> Vec<CallRecord> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let mut magic = [0u8; 2];
    let is_gz = path.to_ascii_lowercase().ends_with(".gz")
        || (file.read(&mut magic).ok() == Some(2) && magic == [0x1f, 0x8b]);
    let Ok(reopened) = File::open(path) else {
        return Vec::new();
    };
    if is_gz {
        let mut reader = BufReader::with_capacity(65_536, MultiGzDecoder::new(reopened));
        read_jsonl_from_bufread(&mut reader, stats)
    } else {
        let mut reader = BufReader::with_capacity(65_536, reopened);
        read_jsonl_from_bufread(&mut reader, stats)
    }
}

pub fn read_jsonl_from_bufread<R: BufRead>(
    reader: &mut R,
    mut stats: Option<&mut ReadStats>,
) -> Vec<CallRecord> {
    let mut raw_line = String::with_capacity(16_384);
    let mut out = Vec::new();
    loop {
        raw_line.clear();
        match reader.read_line(&mut raw_line) {
            Ok(0) => break,
            Ok(n) if n > 32 * 1024 * 1024 => {
                if let Some(ref mut st) = stats {
                    st.bad_lines += 1;
                }
                continue;
            }
            Ok(_) => {}
            Err(_) => {
                if let Some(ref mut st) = stats {
                    st.bad_lines += 1;
                }
                continue;
            }
        }
        let line = raw_line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }
        if !line.contains("\"segments\"") {
            if serde_json::from_str::<Value>(line).is_err() {
                if let Some(ref mut st) = stats {
                    st.bad_lines += 1;
                }
            }
            continue;
        }
        match serde_json::from_str::<CallRecord>(line) {
            Ok(rec) => out.push(rec),
            Err(_) => {
                if let Some(ref mut st) = stats {
                    st.bad_lines += 1;
                }
            }
        }
    }
    out
}

pub fn atomic_write_0600(dest: &str, content: &str) -> std::io::Result<()> {
    let p = Path::new(dest);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = format!("{}.{}.tmp", dest, std::process::id());
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(content.as_bytes())?;
    f.flush()?;
    drop(f);
    if fs::rename(&tmp, dest).is_err() {
        let _ = fs::remove_file(dest);
        if let Err(e) = fs::rename(&tmp, dest) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    }
    Ok(())
}
