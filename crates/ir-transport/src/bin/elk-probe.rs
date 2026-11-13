//! elk-probe - records raw traffic with the ELK Smart IR dongle as fixtures.
//!
//! Exists to capture real identify, learn and transmit exchanges from the
//! hardware so they can become regression tests. After a transmit it listens
//! briefly and logs anything the dongle sends back. Every transfer is logged with its time
//! offset to `fixtures/<unix-time>-<command>[-name].log` in the workspace;
//! only a short summary is printed. The log is written even when a step fails.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ir_transport::ops::{self, LearnConfig, LearnOutcome, LEARN_READ_TIMEOUT};
use ir_transport::transport::{Transport, TransportError};
use ir_transport::probe::{self, ProbeCmd};
use ir_transport::recorder::{self, Recorder};
use ir_transport::usb::UsbTransport;

/// Gap after which a started reply counts as short (in 100 ms reads).
const GAP_READS: u32 = 10;
/// Reads (of 100 ms) to listen for a reply after transmitting.
const LISTEN_READS: u32 = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match probe::parse_args(&args) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("elk-probe: {e}\n{}", probe::USAGE);
            return ExitCode::from(2);
        }
    };

    let transport = match UsbTransport::open() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("elk-probe: {e}");
            return ExitCode::FAILURE;
        }
    };
    let s = &transport.summary;
    let mut header = vec![
        format!("# elk-probe {}", args.join(" ")),
        format!(
            "# device: {:04x}:{:04x} {} bus {} addr {} serial {}",
            ir_transport::usb::VENDOR_ID,
            s.product_id,
            s.product.as_deref().unwrap_or("?"),
            s.bus_id,
            s.address,
            s.serial.as_deref().unwrap_or("?"),
        ),
        format!(
            "# endpoints: out 0x{:02x} in 0x{:02x}",
            transport.endpoints.out_addr, transport.endpoints.in_addr
        ),
    ];
    println!("Opened {}", &header[1][2..]);

    let mut rec = Recorder::new(transport);
    let mut results = Vec::new();
    let ok = run(&cmd, &mut rec, &mut results);

    header.push(format!("# read timeouts (not listed): {}", rec.read_timeouts()));
    let mut text = header.join("\n");
    text.push('\n');
    for entry in rec.entries() {
        text.push_str(&recorder::format_entry(entry));
        text.push('\n');
    }
    for line in &results {
        text.push_str("# ");
        text.push_str(line);
        text.push('\n');
    }

    match write_log(&cmd, &text) {
        Ok(path) => println!("Log written to {}", path.display()),
        Err(e) => {
            eprintln!("elk-probe: could not write log: {e}");
            return ExitCode::FAILURE;
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Runs the command, appending result lines. Returns false on failure.
fn run(cmd: &ProbeCmd, rec: &mut Recorder<UsbTransport>, results: &mut Vec<String>) -> bool {
    let protocol = match ops::identify(rec) {
        Ok(p) => p,
        Err(e) => {
            results.push(format!("identify failed: {e}"));
            println!("Identify failed: {e}");
            println!("If a learn was cancelled earlier, press any remote button at the dongle or replug it.");
            return false;
        }
    };
    results.push(format!("protocol: {protocol:?}"));
    println!("Protocol: {protocol:?}");

    let (wait_s, count) = match cmd {
        ProbeCmd::Identify => return true,
        ProbeCmd::Transmit { payload, freq_hz } => return run_transmit(payload, *freq_hz, rec, results),
        ProbeCmd::Learn { wait_s, count, .. } => (wait_s, count),
    };
    let reads_per_s = (1000 / LEARN_READ_TIMEOUT.as_millis()) as u32;
    let cfg = LearnConfig { wait_reads: wait_s.saturating_mul(reads_per_s), gap_reads: GAP_READS };

    for i in 1..=*count {
        println!("[{i}/{count}] Press a remote button less than 1 cm from the dongle (waiting {wait_s} s)...");
        let outcome = ops::learn(rec, cfg);
        results.push(format!("learn {i}:"));
        match outcome {
            Ok(outcome) => {
                let lines = probe::describe_outcome(&outcome);
                println!("  {}", lines[0]);
                results.extend(lines);
                if outcome == LearnOutcome::NothingReceived {
                    println!("  The dongle may now ignore commands until it sees an IR signal.");
                    return false;
                }
            }
            Err(e) => {
                results.push(format!("learn failed: {e}"));
                println!("  Learn failed: {e}");
                return false;
            }
        }
    }
    true
}

fn run_transmit(
    payload: &probe::Payload,
    freq_hz: u32,
    rec: &mut Recorder<UsbTransport>,
    results: &mut Vec<String>,
) -> bool {
    let pulses = match payload.pulses() {
        Ok(p) => p,
        Err(e) => {
            results.push(format!("transmit failed: {e}"));
            return false;
        }
    };
    if let Err(e) = ops::transmit(rec, freq_hz, &pulses, &mut std::thread::sleep) {
        results.push(format!("transmit failed: {e}"));
        println!("Transmit failed: {e}");
        return false;
    }
    results.push(format!("transmit: {} pulses at {freq_hz} Hz sent", pulses.len()));
    println!("Sent {} pulses at {freq_hz} Hz", pulses.len());

    let mut replies = 0;
    for _ in 0..LISTEN_READS {
        match rec.read(LEARN_READ_TIMEOUT) {
            Ok(_) => replies += 1,
            Err(TransportError::Timeout) => {}
            Err(e) => {
                results.push(format!("listen failed: {e}"));
                return false;
            }
        }
    }
    results.push(format!("transfers received while listening {} ms: {replies}", LISTEN_READS * 100));
    println!("Transfers received after sending: {replies}");
    true
}

fn write_log(cmd: &ProbeCmd, text: &str) -> std::io::Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    fs::create_dir_all(&dir)?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = match cmd {
        ProbeCmd::Identify => format!("{secs}-identify.log"),
        ProbeCmd::Transmit { .. } => format!("{secs}-transmit.log"),
        ProbeCmd::Learn { name: Some(n), .. } => format!("{secs}-learn-{n}.log"),
        ProbeCmd::Learn { name: None, .. } => format!("{secs}-learn.log"),
    };
    let path = dir.join(name);
    fs::write(&path, text)?;
    Ok(path.canonicalize().unwrap_or(path))
}
