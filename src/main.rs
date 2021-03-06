use std::env;
use std::ptr;

// For threading and channels
//use std::thread;
use std::thread::sleep;

use std::time::Duration;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::io::Read;

use std::io::Write;
use std::io::Error;
use std::io::ErrorKind;
//use std::io::prelude::*;

// For driver init
extern crate serial;

//extern crate floating_duration;
//use floating_duration::TimeAsFloat;

extern crate lora_driver;
use lora_driver::RadioConfig;
use lora_driver::Driver;
use lora_driver::RadioMode;

//extern crate libc;
//use libc::termios;
//use libc::winsize;

extern crate nix;
use nix::sys::select::select;
use nix::sys::select::FdSet;
use nix::sys::time::TimeValLike;

// all shit for pty
use nix::pty::openpty;
use nix::libc::tcgetattr;
use nix::libc::tcsetattr;
use nix::libc::termios;

// and another bit for pty function
use std::mem;

// Testing for Bryant
//extern crate serialport;


static CMD_BYTE_HEARTBEAT: u8 = 1;
static CMD_BYTE_RETRY: u8 = 2;

static PACKET_TIMEOUT: u32 = 2_000; //won't be good for 1k
static PTY_TIMEOUT: u32 = 1_000;

static MAX_PACKET_SIZE: usize = 57;

static XON: u8 = 17; // software flow control. dec 19 is XON
static XOFF: u8 = 19; // and 19 is XOFF

// Fills buffer with data from file, and returns the number of bytes read
pub fn read_timeout(file: &mut File, mut buffer: &mut Vec<u8>, duration: u32) -> Result<usize, Error> {
	let file_fd = file.as_raw_fd();
	//println!("read_timeout FD: {:?}", file_fd);
	//let mut blank_fd_set = FdSet::new();
	let mut file_fd_set = FdSet::new();
	file_fd_set.insert(file_fd);
	let duration_i64 = duration as i64;
	let mut timeval_millis = TimeValLike::milliseconds(duration_i64);
	match select(file_fd+1, Some(&mut file_fd_set), None, None, Some(&mut timeval_millis)) {
		Ok(1) => {
			// Ok to read from file

			// debug
			//println!("Select succeeded with a 1");
			
			// Read tup to the max number of bytes to fill the packet (probably 58)
			let bytes_read = file.read(&mut buffer).unwrap();

			Ok(bytes_read)
		}
		Ok(0) => {
			// Timeout
			//println!("Select timed out");
			Err(Error::new(ErrorKind::Other, "Read pty timed out"))
		}
		Ok(_) => {
			// Something else we don't expect
			panic!("Got a weird return code from select");
		}
		Err(e) => {
			// Timeout
			panic!("Select had an error {}",e);
		}
	}
}


// Get a pty pair using direct linux libc syscalls
pub fn create_pty_pair() -> File {

	match openpty(None,None) {
		Ok(result) => {

			// Set our software flow control
			let mut retcode;
			// what I'm doing here is unsafe as shit.
			unsafe {
				// Make the changes to our master terminal
				let mut pty_attr: termios = mem::zeroed();
				
				retcode = tcgetattr(result.master, &mut pty_attr);
				// Set our serial parameters. Flow control, etc.
				// Everything will be off by default to avoid weird default terminal behavior
				// (ECHO is on by default, for example)
				pty_attr.c_iflag = nix::libc::IXOFF +
								   nix::libc::IGNBRK;
				pty_attr.c_oflag = 0;
				pty_attr.c_cflag = 0;
				pty_attr.c_lflag = 0;

				
				retcode = tcsetattr(result.master, 0 ,&pty_attr);
				
				//println!("retcode: {}", retcode);

			}


			let pty_master_file: File;
			unsafe { pty_master_file = File::from_raw_fd(result.master); }
			pty_master_file
		}
		Err(e) => panic!("Failed to open pty master file: {:?}", e)
	}
}

fn main() {

	// Get args that were passed
	let args: Vec<String> = env::args().collect();
	let platform = &args[1];
	let send_or_receive = &args[2];

	let mut e32_driver;
	let mut testconfig = RadioConfig::new();

	match &platform[..] {
		"vocore" => { 
						e32_driver = Driver::new(23,26,24, "/dev/ttyS0");
						//set our serial rate to 57600 as well
						e32_driver.set_tty_params(
							serial::Baud115200,
							serial::Bits8,
							serial::ParityNone,
							serial::Stop1,
							serial::FlowNone
						);
						e32_driver.set_tty_baud(serial::Baud57600);
						testconfig.set_serial_rate("57600");
					}
		"chip" => { 
						e32_driver = Driver::new(1013,1015,1017, "/dev/ttyS0");
						e32_driver.set_tty_params(
							serial::Baud115200,
							serial::Bits8,
							serial::ParityNone,
							serial::Stop1,
							serial::FlowNone
						);
						testconfig.set_serial_rate("115200");
				  }
		_ => panic!("Please enter either 'vocore' or 'chip'")
	}


	/* TESTING!!! LOOP TO CREATE PTY PAIR AND JUST READ FROM IT TO SEE WHAT'S GOING ON 
	let mut pty_master_file = create_pty_pair();
	loop {
		let mut pty_read_buf = vec![0;57];
		// Send XON flow control, we're ready to receive serial data
		//pty_master_file.write_all(&[XON]).unwrap();
		match read_timeout(&mut pty_master_file, &mut pty_read_buf, 5000) {
			Ok(x) => {println!("{:?}", pty_read_buf);}
			Err(x) => {println!("read nothing, iterating");}
		}
		// We've read as much as we can fit into a packet, send transmit off
		//pty_master_file.write_all(&[XOFF]).unwrap();
		sleep(Duration::from_secs(1));
		pty_master_file.write_all(&[69]).unwrap();
	}
	*/
	///*
	testconfig.set_transmit_power("20dBm");
	testconfig.set_air_rate("10k");
	testconfig.set_address("FFFF"); // Channel 0
	e32_driver.set_mode(RadioMode::General);
	e32_driver.write_config(testconfig);
	
	

	let mut pty_master_file = create_pty_pair();
	//let mut bufreader = BufReader::new(pty_master_file);

	/* I don't want to do obect oriented stuff for this loop... might do it later, for now, states
	0: no operation, shouldn't ever be this.
	1: receive packet
	2: flush received to output
	3: read from serial and send packet
	4: send previous packet again
	5: send heartbeat packet
	6: send retry packet
	*/
	let mut state = 0;
	match &send_or_receive[..] {
		"s" => state = 3,
		"r" => state = 1,
		_ => panic!("Invalid option specified, must be 'r' or 's'")
	}

	let mut previous_packet: Vec<u8> = vec![0;MAX_PACKET_SIZE];
	let mut received_packet: Vec<u8> = vec![0;MAX_PACKET_SIZE];
	// Start our operation loop
	loop {

		// for debug slow our whole loop down to 1hz
		//sleep(Duration::from_secs(1));

		match state {
			// receive packet
			1 => {
				println!("Receive packet (1)");
				let packet_read;
				
				match e32_driver.receive_packet(PACKET_TIMEOUT) {
					Ok(bytes_read) => {

						// TODO: this is a temp solution for falsely detecting a packet on the Radio
						if bytes_read.len() == 0 { 
							println!("False positive from radio, trying again.");
							state = 1;
							continue;
						}


						// check to make sure we haven't received a command packet
						if bytes_read[0] == CMD_BYTE_RETRY {
							// Send previous packet again
							println!("Received a retry packet");
							state = 4;
							continue;
						}
						else if bytes_read[0] == CMD_BYTE_HEARTBEAT {
							// Received heartbeat, write no data
							println!("Received a heartbeat packet");
							state = 3;
							continue;
						}
						else if bytes_read[0] == 0 {
							// Not a command byte, continue with the state routine
							packet_read = bytes_read;
						}
						else {
							// Some weird malformed shit, we should never see this
							println!("Corrupt command byte: {:?}, trying again", bytes_read);
							state = 1;
							continue;
						}
					},
					Err(e) => {
						// Timed out, we need to send a retry packet
						state = 6;
						// hop out of this match and continue the loop
						continue;
					}
				}; // don't return, we're not done yet 

				// write out packet to receive_packet, but not the command byte.
				received_packet = packet_read[1..].to_vec();
				state = 2;
				
			}
			// flush received data to output
			2 => {
				println!("flush received data to output (2): {:?}", received_packet);
				if received_packet.len() == 0 { panic!("Attempted to output blank packet, this should never happen"); }
				// send our output without the command byte
				pty_master_file.write_all(&received_packet).expect("Failed to output data to pty");
				state = 3;
			}
			// read from serial and send packet
			// TODO: truncate the packets in the case that we don't get 57 bytes of data
			3 => {
				println!("read from serial and send packet (3)");
				let mut packet_to_send: Vec<u8> = vec![0;MAX_PACKET_SIZE];
				// Send XON flow control, we're ready to receive serial data
				pty_master_file.write_all(&[XON]).unwrap();
				// reads up to 57 bytes from serial
				match read_timeout(&mut pty_master_file, &mut packet_to_send, PTY_TIMEOUT) {
					Ok(bytes_read) => {
						// We read some bytes
						// Truncate the packet if it's smaller than 57
						if bytes_read < MAX_PACKET_SIZE {
							packet_to_send.truncate(bytes_read);
						}
						// set the previous packet here
						previous_packet = packet_to_send.clone();
						// make sure our command byte is in front
						println!("Sending data packet: {:?}", packet_to_send);
						let mut send_packet_w_cmd = vec![0];
						// push our data after the command byte
						send_packet_w_cmd.append(&mut packet_to_send);
						e32_driver.send_packet(&send_packet_w_cmd, 5000).expect("Error sending packet");
						// our turn to read a packet
						state = 1;
					}
					Err(e) => {
						// Got no data from serial. Send heartbeat instead
						state = 5;
					}
				}
			// We've read as much as we can fit into a packet, send transmit off
			pty_master_file.write_all(&[XOFF]).unwrap();
			}
			// send previous packet again
			4 => {
				println!("send previous packet again (4)");
				let mut send_packet_w_cmd = vec![0];
				// push our data after the command byte
				send_packet_w_cmd.append(&mut previous_packet.clone());
				e32_driver.send_packet(&send_packet_w_cmd, 5000).expect("Error sending previous packet");
				// our turn to read a packet
				state = 1;
			}
			// send heartbeat packet
			5 => {
				println!("send heartbeat packet (5)");
				let send_packet_w_cmd = vec![CMD_BYTE_HEARTBEAT];
				e32_driver.send_packet(&send_packet_w_cmd, 5000).expect("Error sending heartbeat packet");
				// our turn to read a packet
				state = 1;
			}
			// send retry packet
			6 => {
				println!("send retry packet (6)");
				let send_packet_w_cmd = vec![CMD_BYTE_RETRY];
				e32_driver.send_packet(&send_packet_w_cmd, 5000).expect("Error sending heartbeat packet");
				// our turn to read a packet
				state = 1;
			}
			_ => panic!("Invalid state... some code is bad")
		}; // we don't want to return from the match to keep the loop going

	}
	//*/
}
