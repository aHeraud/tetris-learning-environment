import platform
import numpy
from cffi import FFI

# This has to point to the directory that holds the shared object files for the library
# TODO: find a better way to locate the library files
# TODO: build for mac os, add support for different architectures (x86 vs just x86_64)
libpath = "../target/release/"

if platform.system() == "Linux":
	libpath += "libtetris_learning_environment.so"
elif platform.system() == "Windows":
	libpath += "tetris_learning_environment.dll"
else:
	raise Exception("unsupported system")

ffi = FFI()
ffi.set_source("_environment_cffi", None)

# ideally we would just read in the header file we generated with cbindgen,
# but cffi doesn't support preprocessor stuff yet
# with open(HEADER_PATH, "r") as header:
# 	contents = header.read()
# 	ffi.cdef(contents)

ffi.cdef('''
	typedef enum {
		Up = 0, Down = 1, Left = 2, Right = 3, B = 4, A = 5, Select = 6, Start = 7
	} Key;
	typedef struct Environment Environment;
	void destroy_environment(Environment *env_ptr);
	const uint32_t *get_pixels(Environment *env_ptr);
	int32_t get_score(const Environment *env_ptr);
	Environment *initialize_environment(const char *rom_path_ptr);
	bool is_running(const Environment *env_ptr);
	void run_frame(Environment *env_ptr);
	void set_key_state(Environment *env_ptr, Key key, bool pressed);
	int32_t start_episode(Environment *env_ptr);
''')
lib = ffi.dlopen(libpath)


class Environment:
	WIDTH = 160
	HEIGHT = 144

	def __init__(self, rom_path: str):
		rom_path_utf8 = rom_path.encode(encoding="UTF-8")
		self.__obj = lib.initialize_environment(rom_path_utf8)
		if self.__obj == ffi.NULL:
			raise Exception("failed to initialize environment (maybe the rom path was incorrect?)")

	def __del__(self):
		# free environment memory
		if self.__obj != ffi.NULL:
			lib.destroy_environment(self.__obj)
		self.__obj = None

	def start_episode(self):
		lib.start_episode(self.__obj)

	def run_frame(self):
		lib.run_frame(self.__obj)

	def is_running(self) -> bool:
		return lib.is_running(self.__obj)

	# key is a key enum exported by the rust library
	# the variants are [Up, Down, Left, Right, B, A, Select, Start]
	# you can access the enums through the lib variable, for example: `key_up = lib.Up`
	def set_key_state(self, key, pressed: bool):
		lib.set_key_state(self.__obj, key, pressed)

	def get_score(self) -> int:
		return lib.get_score(self.__obj)

	def get_pixels(self) -> numpy.ndarray:
		# this still needs to be tested
		buffer = ffi.buffer(lib.get_pixels(self.__obj), self.WIDTH * self.HEIGHT * 4)
		return numpy.frombuffer(buffer, dtype="int32", count=self.WIDTH * self.HEIGHT)


# a little demo
if __name__ == "__main__":
	import time
	env = Environment("X:/Roms/Gameboy/Tetris (W) (V1.1) [!].gb")

	# Benchmark
	start = time.time()
	env.start_episode()
	frames = 0
	while env.is_running():
		env.run_frame()
		frames += 1
	end = time.time()
	print("emulated game in " + str(end - start) + " seconds (" + str(frames) + " frames)")
