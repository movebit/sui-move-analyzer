// This module is a demo on "macro functions"
module Move2024Demo::m44 {
    macro fun call($f: |u64| -> u64, $x: u64): u64 {
        // $f = 0;
        // $x = 0;
        // $f($x)
        1
    }

    fun t() {
        // ensure the macro is expanded
        call!(|_| false, 0) + 1;
    }
}
