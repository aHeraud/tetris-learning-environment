extern crate agb_core;
extern crate rand;

use std::os::raw::c_char;
use std::sync::atomic::AtomicBool;
use std::collections::HashMap;
use std::path::Path;
use std::convert::AsRef;
use std::error::Error;
use std::sync::Arc;
use std::ptr;

use agb_core::Gameboy;
pub use agb_core::{Key, WIDTH, HEIGHT};

const GAME_START_STATE: &'static [u8] = include_bytes!("game_start.state");

pub struct Environment {
	gameboy: Box<Gameboy>,
	keys: HashMap<Key, bool>,
	running: Arc<AtomicBool>
}

impl Environment {
	pub fn new<P: AsRef<Path>>(rom_path: P) -> Result<Environment, Box<Error>> {
		use std::fs::File;
		use std::io::Read;

		let mut buf: Vec<u8> = Vec::new();
		let mut rom_file = File::open(rom_path)?;
		rom_file.read_to_end(&mut buf)?;
		let gameboy = Box::new(Gameboy::new(buf.into_boxed_slice(), None)?);

		let keys: HashMap<Key, bool> = [
			(Key::A, false),
			(Key::B, false),
			(Key::Select, false),
			(Key::Start, false),
			(Key::Up, false),
			(Key::Down, false),
			(Key::Left, false),
			(Key::Right, false)
		].iter().cloned().collect();

		Ok(Environment {
			gameboy,
			keys,
			running: Arc::new(AtomicBool::from(false))
		})
	}
}

/// Initialize the emulation environment.
///
/// ARGS:
///     rom_path_ptr: the path to the Tetris rom, must be a valid UTF-8 string.
///
/// Returns:
///     A pointer to the environment struct, or null if there was an error.
#[no_mangle]
pub unsafe extern "C" fn initialize_environment(rom_path_ptr: *const c_char) -> *mut Environment {
	use std::ffi::CStr;

	use agb_core::gameboy::debugger::{DebuggerInterface, Breakpoint, AccessType};

	// paths on windows and unix are very different so it's easier to just pray that whatever string we get
	// is a valid utf-8 string
	let rom_path = match CStr::from_ptr(rom_path_ptr).to_str() {
		Ok(string) => string,
		Err(_e) => {
			println!("rom path not a valid utf-8 string");
			return ptr::null_mut();
		}
	};

	match Environment::new(rom_path) {
		Ok(mut env) => {
			// set end of game breakpoint
			env.gameboy.add_breakpoint(Breakpoint::new(0x6803, AccessType::Execute));
			env.gameboy.clear_breakpoint_callback();
			{
				let running = env.running.clone();
				env.gameboy.register_breakpoint_callback(move |_bp| {
					use std::sync::atomic::Ordering;
					running.store(false, Ordering::Relaxed); // sets environment->running = false
				});
			}
			Box::into_raw(Box::new(env))
		},
		Err(e) => {
			println!("Failed to initialize environment: {}", e);
			ptr::null_mut()
		}
	}
}

/// Free the resources used by the environment.
///
/// When this is called, the memory allocated to hold the environment is freed, and
/// any pointers to the pixel data handed out by get_pixels become invalid.
#[no_mangle]
pub unsafe extern "C" fn destroy_environment(env_ptr: *mut Environment) {
	// take ownership of the pointer, when the function returns the box goes out of scope and is destroyed
	if env_ptr != ptr::null_mut() {
		let _env = Box::from_raw(env_ptr);
	}
}

/// Start a new game of Tetris.
///
/// This loads the game start state, and re-seeds the prng.
#[no_mangle]
pub unsafe extern "C" fn start_episode(env_ptr: *mut Environment) -> i32 {
	use std::sync::atomic::Ordering;
	use rand::thread_rng;
	use rand::Rng;
	use agb_core::gameboy::debugger::{DebuggerInterface};

	if env_ptr == ptr::null_mut() {
		return -1;
	}

	let environment = &mut *env_ptr;
	if environment.gameboy.load_state(GAME_START_STATE).is_err() {
		return -1;
	}

	// seed prng by randomizing contents of div register
	let seed = thread_rng().gen_range(0, 0xFFFF);
	environment.gameboy.set_div(seed);

	environment.running.store(true, Ordering::Relaxed);

	0
}

/// Run a single frame of the game
#[no_mangle]
pub unsafe extern "C" fn run_frame(env_ptr: *mut Environment) {
	use std::sync::atomic::Ordering;
	use std::time::Duration;
	use agb_core::FPS;

	if env_ptr == ptr::null_mut() {
		return;
	}

	let environment = &mut *env_ptr;

	if environment.running.load(Ordering::Relaxed) {
		// emulates one frame (~1/60 of a second)
		environment.gameboy.emulate(Duration::from_millis((1000.0f64 / FPS) as u64));
	}
}

/// Returns a pointer to the beginning of the frame buffer of the emulator, which holds the contents of the screen.
///
/// The array holds WIDTH * HEIGHT pixels, where each pixel is a 32-bit RGBA integer.
///
/// The buffer referenced by the returned pointer is invalidated after the next time run_frame is called.
#[no_mangle]
pub unsafe extern "C" fn get_pixels(env_ptr: *mut Environment) -> *const u32 {
	if env_ptr == ptr::null_mut() {
		ptr::null_mut()
	}
	else {
		let environment = &mut *env_ptr;
		environment.gameboy.get_framebuffer().as_ptr()
	}
}

#[no_mangle]
pub unsafe extern "C" fn set_key_state(env_ptr: *mut Environment, key: Key, pressed: bool) {
	if env_ptr == ptr::null_mut() {
		return;
	}

	let environment = &mut *env_ptr;

	if let Some(state) = environment.keys.get_mut(&key) {
		if *state != pressed {
			if pressed {
				environment.gameboy.keydown(key);
			}
			else {
				environment.gameboy.keyup(key);
			}
			*state = pressed;
		}
	}
}

/// Get the score from a game of tetris that just ended.
/// The score is stored as a 3-byte little endian bcd at address 0xC0A0
#[no_mangle]
pub unsafe extern "C" fn get_score(env_ptr: *const Environment) -> i32 {
	use agb_core::gameboy::debugger::DebuggerInterface;

	if env_ptr == ptr::null_mut() {
		return -1;
	}

	let environment = & *env_ptr;

	let score_bcd = [environment.gameboy.read_memory(0xC0A2), environment.gameboy.read_memory(0xC0A1), environment.gameboy.read_memory(0xC0A0)];

	let mut score: i32 = 0;
	for i in 0..3 {
		let high_digit = ((score_bcd[i] & 0xF0) >> 4) as i32;
		let low_digit = ((score_bcd[i]) & 0x0F) as i32;

		score += high_digit * (10 * ((2 * (i as i32)) + 1));
		score += low_digit * (10 * (2 * (i as i32)));
	}

	score
}
