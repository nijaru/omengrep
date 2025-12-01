from sys import external_call
from memory import UnsafePointer, alloc

alias REG_EXTENDED = 1
alias REG_ICASE    = 2
alias REG_NOSUB    = 4
alias REG_NEWLINE  = 8
alias REGEX_T_SIZE = 128
alias CInt = Int32

alias VoidPtr = Int

fn regcomp(preg: VoidPtr, pattern: VoidPtr, cflags: CInt) -> CInt:
    return external_call["regcomp", CInt](preg, pattern, cflags)

fn regexec(preg: VoidPtr, string: VoidPtr, nmatch: Int, pmatch: VoidPtr, eflags: CInt) -> CInt:
    return external_call["regexec", CInt](preg, string, nmatch, pmatch, eflags)

fn regfree(preg: VoidPtr):
    external_call["regfree", NoneType](preg)

struct Regex:
    var _preg: Int
    var _pattern: String
    var _initialized: Bool

    fn __init__(out self, pattern: String):
        var ptr = alloc[UInt8](REGEX_T_SIZE)
        self._preg = Int(ptr)
        self._pattern = pattern
        self._initialized = False
        
        var p_copy = pattern
        var c_pattern = Int(p_copy.unsafe_cstr_ptr())
        
        var ret = regcomp(self._preg, c_pattern, CInt(REG_EXTENDED | REG_NOSUB | REG_ICASE))
        if ret == 0:
            self._initialized = True
        else:
            print("Regex compilation failed for: " + pattern)

    fn __moveinit__(out self, owned existing: Self):
        self._preg = existing._preg
        self._pattern = existing._pattern
        self._initialized = existing._initialized
        existing._initialized = False

    fn __del__(owned self):
        if self._initialized:
            regfree(self._preg)
            # TODO: Free the _preg memory (128 bytes)
            # Currently leaked because reconstructing UnsafePointer from Int
            # hits type inference issues in v0.25.7.
            pass

    fn matches(self, text: String) -> Bool:
        if not self._initialized:
            return False
            
        var t_copy = text
        var c_text = Int(t_copy.unsafe_cstr_ptr())
        
        var dummy = alloc[UInt8](1)
        var dummy_addr = Int(dummy)
        
        var ret = regexec(self._preg, c_text, 0, dummy_addr, CInt(0))
        
        dummy.free()
        return ret == 0