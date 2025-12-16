from sys import external_call
from memory import UnsafePointer, alloc

alias REG_EXTENDED = 1
alias REG_ICASE = 2
alias REG_NOSUB = 4
alias REG_NEWLINE = 8
alias REGEX_T_SIZE = 128
alias CInt = Int32

alias VoidPtr = Int


fn regcomp(preg: VoidPtr, pattern: VoidPtr, cflags: CInt) -> CInt:
    return external_call["regcomp", CInt](preg, pattern, cflags)


fn regexec(
    preg: VoidPtr, string: VoidPtr, nmatch: Int, pmatch: VoidPtr, eflags: CInt
) -> CInt:
    return external_call["regexec", CInt](preg, string, nmatch, pmatch, eflags)


fn regfree(preg: VoidPtr):
    external_call["regfree", NoneType](preg)


struct Regex:
    var _preg: UnsafePointer[UInt8, MutOrigin.external]
    var _pattern: String
    var _initialized: Bool

    fn __init__(out self, pattern: String):
        self._preg = alloc[UInt8](REGEX_T_SIZE)
        self._pattern = pattern
        self._initialized = False

        var p_copy = pattern
        var c_pattern = Int(p_copy.unsafe_cstr_ptr())

        var ret = regcomp(
            Int(self._preg),
            c_pattern,
            CInt(REG_EXTENDED | REG_NOSUB | REG_ICASE),
        )
        if ret == 0:
            self._initialized = True
        else:
            print("Regex compilation failed for: " + pattern)

    fn __moveinit__(out self, deinit existing: Self):
        self._preg = existing._preg
        self._pattern = existing._pattern^
        self._initialized = existing._initialized
        # Prevent double-free: null out pointer before existing's destructor runs
        __mlir_op.`lit.ownership.mark_destroyed`(
            __get_mvalue_as_litref(existing)
        )

    fn __del__(deinit self):
        if self._initialized:
            regfree(Int(self._preg))
        if self._preg:
            self._preg.free()

    fn matches(self, text: String) -> Bool:
        if not self._initialized:
            return False

        var t_copy = text
        var c_text = Int(t_copy.unsafe_cstr_ptr())

        var dummy = alloc[UInt8](1)
        var ret = regexec(Int(self._preg), c_text, 0, Int(dummy), CInt(0))
        dummy.free()

        return ret == 0
