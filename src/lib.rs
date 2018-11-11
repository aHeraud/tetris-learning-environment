extern crate agb_core;
extern crate rand;

use std::os::raw::c_char;
//use std::sync::atomic::AtomicBool;

use agb_core::Gameboy;

const GAME_START_STATE: &'static [u8] = include_bytes!("game_start.state");

//pub struct Environment {
//	gameboy: Gameboy,
//	keys: HashMap<Key, bool>,
//	running: AtomicBool
//}

#[no_mangle]
pub unsafe extern "C" fn initialize_environment(rom_path_ptr: *const c_char) -> *mut Gameboy {
	use std::ffi::CStr;
	use std::fs::File;
	use std::io::Read;
	use std::ptr;

	use agb_core::gameboy::debugger::{DebuggerInterface, Breakpoint, AccessType};

	// paths on windows and unix are very different so it's easier to just pray that whatever string we get
	// is a valid utf-8 string
	let rom_path = CStr::from_ptr(rom_path_ptr).to_str().expect("rom path not a valid utf-8 string");
	let mut rom_file = match File::open(&rom_path) {
		Ok(file) => file,
		Err(e) => {
			// failed to open the file
			println!("AGB: failed to open rom file with error: {}", e);
			return ptr::null_mut();
		}
	};

	let mut buffer: Vec<u8> = Vec::new();
	if rom_file.read_to_end(&mut buffer).is_err() {
		println!("AGB: failed to read rom file");
		return ptr::null_mut();
	}

	match Gameboy::new(buffer.into_boxed_slice(), None) {
		Ok(mut gameboy) => {
			// Set a breakpoint at 0x6803 to detect the end of the game
			gameboy.add_breakpoint(Breakpoint::new(0x6803, AccessType::Execute));
			Box::into_raw(Box::new(gameboy))
		},
		Err(e) => {
			println!("AGB: {}", e);
			return ptr::null_mut();
		}
	}
}

pub unsafe extern "C" fn destroy_environment(gameboy_ptr: *mut Gameboy) {
	// take ownership of the pointer, when the function returns the box goes out of scope and is destroyed
	let _gameboy = Box::from_raw(gameboy_ptr);
}

#[no_mangle]
pub unsafe extern "C" fn start_episode(gameboy_ptr: *mut Gameboy) -> i32 {
	use rand::thread_rng;
	use rand::Rng;
	use agb_core::gameboy::debugger::{DebuggerInterface};

	let gameboy = &mut *gameboy_ptr;
	if gameboy.load_state(GAME_START_STATE).is_err() {
		return -1;
	}

	// seed prng by randomizing contents of div register
	let seed = thread_rng().gen_range(0, 0xFFFF);
	gameboy.set_div(seed);

	0
}

#[no_mangle]
pub unsafe extern "C" fn run_frame(gameboy_ptr: *mut Gameboy) {
	use std::time::Duration;
	use agb_core::FPS;

	let gameboy = &mut *gameboy_ptr;

	// TODO: set up breakpoint callback somewhere so we can tell when the game is over

	// emulates one frame (~1/60 of a second)
	gameboy.emulate(Duration::from_millis((1000.0f64 / FPS) as u64));
}

#[no_mangle]
pub unsafe extern "C" fn key_down(gameboy_ptr: *mut Gameboy, key: u32) {

}

#[no_mangle]
pub unsafe extern "C" fn key_up(gameboy_ptr: *mut Gameboy, key: u32) {

}

/// Get the score from a game of tetris that just ended
/// The score is stored as a 3-byte little endian bcd at address 0xC0A0
#[no_mangle]
pub unsafe extern "C" fn get_score(gameboy_ptr: *const Gameboy) -> i32 {
	use agb_core::gameboy::debugger::DebuggerInterface;

	let gameboy = & *gameboy_ptr;

	let score_bcd = [gameboy.read_memory(0xC0A2), gameboy.read_memory(0xC0A1), gameboy.read_memory(0xC0A0)];

	let mut score: i32 = 0;
	for i in 0..3 {
		let high_digit = ((score_bcd[i] & 0xF0) >> 4) as i32;
		let low_digit = ((score_bcd[i]) & 0x0F) as i32;

		score += high_digit * (10 * ((2 * (i as i32)) + 1));
		score += low_digit * (10 * (2 * (i as i32)));
	}

	score
}
