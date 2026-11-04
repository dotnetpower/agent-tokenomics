use crate::model::CallRecord;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

pub struct JsonlWriter {
    pub path: String,
    inner: Mutex<Option<BufWriter<File>>>,
}

impl JsonlWriter {
    pub fn new(path: impl Into<String>) -> std::io::Result<Self> {
        let p = path.into();
        if let Some(parent) = Path::new(&p).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut opts = OpenOptions::new();
        opts.create(true).append(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(&p)?;
        Ok(Self {
            path: p,
            inner: Mutex::new(Some(BufWriter::with_capacity(65_536, file))),
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
            if let Some(ref mut w) = *guard {
                let _ = w.write_all(line.as_bytes());
                let _ = w.write_all(b"\n");
                let _ = w.flush();
            }
        }
    }

    pub fn close(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(mut w) = guard.take() {
                let _ = w.flush();
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

pub fn read_jsonl(path: &str, mut stats: Option<&mut ReadStats>) -> Vec<CallRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line_res in reader.lines() {
        let Ok(raw_line) = line_res else {
            if let Some(ref mut st) = stats {
                st.bad_lines += 1;
            }
            continue;
        };
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
