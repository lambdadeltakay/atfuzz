use clap::Parser;
use rand::seq::IteratorRandom;
use rand::{seq::SliceRandom, thread_rng, Rng};
use serialport::SerialPort;
use simple_logger::SimpleLogger;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

const AT_COMMAND_SPLITERS: &[&[u8]] = &[b"", b"+", b"%", b"!", b"$", b"#", b"^", b"*"];

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The device to target
    #[arg(short, long)]
    device: String,

    // Replay our successes
    #[arg(short, long)]
    replay: bool,
}

fn get_random_dictionary_word() -> Vec<u8> {
    let f = File::open("dictionary.txt").unwrap();
    let f = BufReader::new(f);
    let line = f
        .lines()
        .map(|l| l.unwrap())
        .choose(&mut rand::thread_rng())
        .unwrap();

    return line.to_ascii_uppercase().into_bytes();
}

fn make_random_at_command() -> Vec<u8> {
    // The basic command
    let mut command = b"AT".to_vec();
    // Add Splitter
    command.extend(
        AT_COMMAND_SPLITERS
            .choose(&mut thread_rng())
            .cloned()
            .unwrap(),
    );

    let payload_length = 1000;

    // Insert random word
    if thread_rng().gen_bool(0.50) {
        command.extend(get_random_dictionary_word());
    }

    // Make sure its not too long
    command.truncate(payload_length);

    // extend it to our length
    command.extend((command.len()..payload_length).map(|_| {
        // Don't do the escape sequence lol
        let mut result = None;
        while result.is_none() || result.unwrap() == b'\r' {
            result = Some(thread_rng().gen::<u8>());
        }
        return result.unwrap();
    }));

    // Make sure its the same size
    assert_eq!(command.len(), payload_length);

    return command;
}

fn send_command(port: &mut Box<dyn SerialPort>, command: &[u8]) -> Option<Vec<u8>> {
    // Allocate buffer
    let mut buffer = vec![0; 64 + command.len()];

    // Write out command
    port.write_all(&[command, b"\r"].concat()).is_ok();

    // Read response
    if port.read(&mut buffer).is_err() {
        return None;
    }

    // Return
    return Some(buffer[0..64].to_vec());
}

fn fuzz(mut handle: &mut Box<dyn SerialPort>) {
    loop {
        let random_command = make_random_at_command();

        // Rn idc about the output
        let output = send_command(&mut handle, &random_command);

        // If it all worked out we are not doing great
        if output.is_some() {
            log::info!(
                "Stable for command: {}",
                String::from_utf8(
                    random_command
                        .into_iter()
                        .flat_map(|b| std::ascii::escape_default(b))
                        .collect::<Vec<u8>>(),
                )
                .unwrap()
            )
        } else {
            log::error!(
                "Command crashed the device!: {} ",
                String::from_utf8(
                    random_command
                        .iter()
                        .flat_map(|b| std::ascii::escape_default(*b))
                        .collect::<Vec<u8>>(),
                )
                .unwrap(),
            );

            let mut success_log = OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open("success.txt")
                .expect("Cannot write success");

            writeln!(
                success_log,
                "{}",
                String::from_utf8(
                    random_command
                        .iter()
                        .flat_map(|b| std::ascii::escape_default(*b))
                        .collect::<Vec<u8>>(),
                )
                .unwrap()
            )
            .expect("Cannot write success");

            break;
        }
    }
}

fn replay(mut handle: &mut Box<dyn SerialPort>) {
    if !PathBuf::from("success.txt").exists() {
        log::error!("No success file");
        return;
    }

    let mut codes = Default::default();
    File::open("success.txt")
        .unwrap()
        .read_to_string(&mut codes)
        .unwrap();

    let good_code = codes.lines().into_iter().find(|code| {
        let unescaped_code = smashquote::unescape_bytes(&code.bytes().collect::<Vec<_>>());

        if unescaped_code.is_err() {
            return false;
        }

        return send_command(&mut handle, &unescaped_code.unwrap()).is_none();
    });

    if good_code.is_some() {
        log::info!("Code works: {}", good_code.unwrap());
    } else {
        log::error!("None of the codes worked");
    }
}

fn main() {
    SimpleLogger::new().init().unwrap();

    let cli = Args::parse();

    let mut selected_port = None;
    for p in serialport::available_ports().expect("No ports found!") {
        if p.port_name == cli.device {
            selected_port = Some(p.port_name);
            break;
        }
    }

    if selected_port.is_none() {
        log::error!("Could not find device: {}", cli.device);
        exit(1);
    }

    if selected_port.is_some() {
        let mut handle = serialport::new(&selected_port.unwrap(), 115_200)
            .timeout(Duration::from_secs(10))
            .open()
            .expect("Failed to open port");

        if cli.replay {
            replay(&mut handle);
        } else {
            fuzz(&mut handle);
        }
    }
}
