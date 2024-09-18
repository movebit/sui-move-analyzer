module 0x42::M22 {
    public struct Foo(u64) has copy, drop;

    fun x() {
        let _x = Foo(0);
        abort 0
    }
}
