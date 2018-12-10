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
use std::process::abort;

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
		use agb_core::gameboy::debugger::{DebuggerInterface, Breakpoint, AccessType};

		let mut buf: Vec<u8> = Vec::new();
		let mut rom_file = File::open(rom_path)?;
		rom_file.read_to_end(&mut buf)?;

		let running = Arc::new(AtomicBool::from(false));

		let mut gameboy = Box::new(Gameboy::new(buf.into_boxed_slice(), None)?);
		// set end of game breakpoint
		gameboy.debugger.enable();
		gameboy.add_breakpoint(Breakpoint::new(0x6803, AccessType::Execute));
		{
			let running = running.clone();
			gameboy.register_breakpoint_callback(move |_bp| {
				use std::sync::atomic::Ordering;
				running.store(false, Ordering::Relaxed); // sets environment->running = false
			});
		}

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
			running
		})
	}

	pub fn start_episode(&mut self) -> Result<(), Box<Error>> {
		use std::sync::atomic::Ordering;
		use rand::{thread_rng, Rng};
		use agb_core::gameboy::debugger::{DebuggerInterface};

		self.gameboy.load_state(GAME_START_STATE)?;
		let seed = thread_rng().gen_range(0, 0xFFFF);
		self.gameboy.set_div(seed);
		self.running.store(true, Ordering::Relaxed);

		Ok(())
	}

	pub fn run_frame(&mut self) {
		use std::time::Duration;
		use agb_core::FPS;

		if self.is_running() {
			// emulates the game for ~1/60th of a second
			let ms = (1000.0 / FPS) as u64;
			self.gameboy.emulate(Duration::from_millis(ms));
		}
	}

	pub fn is_running(&self) -> bool {
		use std::sync::atomic::Ordering;

		self.running.load(Ordering::Relaxed)
	}

	/// borrow the contents of the framebuffer
	/// the framebuffer is a 160*144 array of RGBA pixels stored as u32s
	pub fn get_pixels<'a>(&'a self) -> &'a[u32] {
		self.gameboy.get_framebuffer()
	}

	pub fn get_score(&self) -> i32 {
		use agb_core::gameboy::debugger::DebuggerInterface;

		let score_bcd = [self.gameboy.read_memory(0xC0A2), self.gameboy.read_memory(0xC0A1), self.gameboy.read_memory(0xC0A0)];

		let mut score: i32 = 0;
		for i in 0..3 {
			score *= 100; // shift over 2 decimal places

			let high_digit = ((score_bcd[i] & 0xF0) >> 4) as i32;
			let low_digit = ((score_bcd[i]) & 0x0F) as i32;

			score += (high_digit * 10) + low_digit;
		}

		score
	}

	/// Returns the number of lines cleared in the current game (undefined if there is not a game in progress)
	/// The number of lines cleared is stored in HRAM at addresses 0xFF9E, 0xFF9F as a little endian BCD.
	pub fn get_lines(&self) -> i32 {
		use agb_core::gameboy::debugger::DebuggerInterface;

		let mut lines = 0;
		let lines_bcd = [self.gameboy.read_memory(0xFF9F), self.gameboy.read_memory(0xFF9E)];

		for i in 0..2 {
			lines *= 100; // shift over 2 decimal places

			let high_digit = ((lines_bcd[i] & 0xF0) >> 4) as i32;
			let low_digit = ((lines_bcd[i]) & 0x0F) as i32;

			lines += (high_digit * 10) + low_digit;
		}

		lines
	}

	pub fn set_key_state(&mut self, key: Key, pressed: bool) {
		if let Some(state) = self.keys.get_mut(&key) {
			if *state != pressed {
				if pressed {
					self.gameboy.keydown(key);
				}
				else {
					self.gameboy.keyup(key);
				}
				*state = pressed;
			}
		}
	}

	/// Returns an array of RGB bytes (each component is 8-bits)
	pub fn rgb_pixels(&self) -> Box<[u8]> {
		let rgba = self.gameboy.get_framebuffer();

		let mut rgb = Vec::with_capacity(rgba.len() * 3);
		for pixel in rgba {
			rgb.push((*pixel >> 24) as u8); //red component
			rgb.push(((*pixel >> 16) & 0xFF) as u8); //green component
			rgb.push(((*pixel >> 8) & 0xFF) as u8); //blue component
		}

		rgb.into_boxed_slice()
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

	if rom_path_ptr == ptr::null() {
		abort();
	}

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
		Ok(env) => Box::into_raw(Box::new(env)),
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
	if env_ptr == ptr::null_mut() {
		abort();
	}

	let environment = &mut *env_ptr;
	if environment.start_episode().is_err() {
		-1
	}
	else {
		0
	}
}

/// Run a single frame of the game
#[no_mangle]
pub unsafe extern "C" fn run_frame(env_ptr: *mut Environment) {
	if env_ptr == ptr::null_mut() {
		abort();
	}

	let environment = &mut *env_ptr;
	environment.run_frame();
}

/// Whether or not the game is currently still in progress, if false then the game has ended
#[no_mangle]
pub unsafe extern "C" fn is_running(env_ptr: *const Environment) -> bool {
	if env_ptr == ptr::null() {
		abort();
	}
	else {
		let env = &*env_ptr;
		env.is_running()
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
		abort();
	}
	else {
		let environment = &mut *env_ptr;
		environment.get_pixels().as_ptr()
	}
}

/// Returns a pointer to an array holding WIDTH * LENGTH rgb pixels (each component is 8-bits)
/// the length of the array is WIDTH * LENGTH * 3 bytes
/// the array returned by this function must be freed by the free_rgb_pixel_array function
#[no_mangle]
pub unsafe extern "C" fn get_rgb_pixels(env_ptr: *mut Environment) -> *mut u8 {
	// TODO: pass length by reference
	if env_ptr == ptr::null_mut() {
		abort();
	}
	else {
		let environment = & *env_ptr;
		let slice = Box::into_raw(environment.rgb_pixels());
		let s: &mut[u8] = &mut*slice;
		s.as_mut_ptr()
	}
}

#[no_mangle]
pub unsafe extern "C" fn free_rgb_pixel_array(buffer: *mut u8) {
	use std::slice;
	let s = slice::from_raw_parts_mut(buffer, WIDTH * HEIGHT * 3);
	Box::from_raw(s);
}

#[no_mangle]
pub unsafe extern "C" fn set_key_state(env_ptr: *mut Environment, key: Key, pressed: bool) {
	if env_ptr == ptr::null_mut() {
		abort();
	}

	let environment = &mut *env_ptr;
	environment.set_key_state(key, pressed);
}

/// Get the score from a game of tetris that just ended.
/// The score is stored as a 3-byte little endian bcd at address 0xC0A0
#[no_mangle]
pub unsafe extern "C" fn get_score(env_ptr: *const Environment) -> i32 {
	if env_ptr == ptr::null_mut() {
		abort();
	}

	let environment = & *env_ptr;
	environment.get_score()
}

/// Get the number of lines cleared during the current game
#[no_mangle]
pub unsafe extern "C" fn get_lines(env_ptr: *const Environment) -> i32 {
	if env_ptr == ptr::null_mut() {
		abort();
	}

	let environment = & *env_ptr;
	environment.get_lines()
}
