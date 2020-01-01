#![feature(generators, generator_trait)]
use std::iter::ExactSizeIterator;
use std::process;
use libc;

fn exec<I: IntoIterator<Item = V>, V: AsRef<std::ffi::OsStr>>(argv: I) -> impl Iterator<Item = String> {
	let mut it = argv.into_iter();
	use process::{Stdio, Command};
	let mut cmd = Command::new(it.next().unwrap());
	let proc = match cmd.args(it).stdout(Stdio::piped()).stderr(Stdio::inherit()).stdin(Stdio::null()).spawn() {
		Ok(k) => k,
		Err(e) => {
			eprintln!("Failed to exec docker: {}", e);
			process::exit(1);
		}
	};
	let mut proc = Box::new(proc);
	use std::io::{BufReader, BufRead};
	use std::iter;
	let mut g = move || {
		let mut bw = BufReader::new(proc.stdout.as_mut().unwrap());
		let mut line = String::new();
		loop {
			line.clear();
			let res = bw.read_line(&mut line);
			match res {
				Ok(size) => {
					if size > 0 {
						yield line.clone();
					} else {
						match proc.wait() {
							Ok(r) => {
								if !r.success() {
									process::exit(1);
								} else {
									return;
								}
							},
							Err(e) => {
								eprintln!("exec: {}", e);
								process::exit(1);
							}
						}
					}
				},
				Err(e) => {
					eprintln!("Failed to read process output: {}", e);
					unsafe {libc::kill(proc.id() as i32, libc::SIGTERM)};
					process::exit(1);
				}
			}
		}
	};
	use std::ops::{Generator, GeneratorState};
	iter::from_fn(move || {
		match std::pin::Pin::new(&mut g).resume() {
			GeneratorState::Yielded(s) => Some(s),
			GeneratorState::Complete(()) => None
		}
	})
}

fn main() {
	if std::env::args_os().len() > 1 {
		eprintln!("Command line arguments are not supported.");
		process::exit(1);
	}

	use std::collections::HashSet;
	let mut images = HashSet::new();

	for image_line in exec(&["docker", "image", "ls", "--no-trunc", "--format", "{{.ID}}\t{{.Repository}}\t{{.Tag}}"]) {
		let cols: Vec<&str> = image_line.trim_end_matches('\n').split('\t').collect();
		if cols[1] == "<none>" || cols[2] == "<none>" {
			continue;
		}
		images.insert((cols[1].to_owned(), cols[2].to_owned()));
	}

	use process::{Command, Stdio};

	for (image, tag) in images.into_iter() {
		println!("Updating {}:{}", image, tag);
		match Command::new("docker").args(&["pull", "-q", &format!("{}:{}", image, tag)])
			.stderr(Stdio::inherit()).stdout(Stdio::null()).stdin(Stdio::null())
			.status() {
			Ok(r) => {
				if !r.success() {
					eprintln!("docker pull failed for {}:{}.", image, tag);
				}
			},
			Err(e) => {
				eprintln!("unable to get return value: {}", e);
			}
		}
	}
}
