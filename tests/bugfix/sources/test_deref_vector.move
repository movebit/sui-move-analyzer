module a::test_deref_vector {
    // this code from typus:
    // https://github.com/Typus-Lab/typus/blob/d258bade161986af59bbdbc205d9454d2080b81a/typus_framework/sources/vault.move#L2294
    public fun get_bid_receipt_info(): (u64, vector<u64>) {
        let prim_vec: vector<u64> = vector::empty();
        (1, prim_vec)
    }
}

module b::test_deref_vector {
    use a::test_deref_vector::get_bid_receipt_info;
    public fun test() {
        let (vid, share_u64_padding) = get_bid_receipt_info();
        // bug: can't goto definition of `borrow`
        let size = *share_u64_padding.borrow(0); // share
    }
}
