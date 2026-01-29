// Copyright (c) Typus Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements a `KeyedBigVector`, a data structure that combines the features of a `BigVector`
/// and a `Table`. It allows for both indexed and keyed access to a large number of elements by storing
/// them in slices, while maintaining a mapping from keys to indices in a `Table`.
module typus::keyed_big_vector {
    use std::type_name::{Self, TypeName};

    use sui::dynamic_field;
    use sui::table;

    // ======== Constants ========

    /// The maximum number of slices allowed in a KeyedBigVector.
    const CMaxSliceAmount: u16 = 1000;
    /// The maximum size of a slice.
    const CMaxSliceSize: u32 = 262144;
    /// The key for the dynamic field that stores the key-to-index table.
    const SKeyIndexTable: vector<u8> = b"key_index_table";

    // ======== Errors ========

    /// Error for a duplicate key.
    fun duplicate_key(): u64 { abort 0 }
    /// Error for an out-of-bounds index.
    fun index_out_of_bounds(): u64 { abort 0 }
    /// Error for an invalid slice size.
    fun invalid_slice_size(): u64 { abort 0 }
    /// Error when a key is not found.
    fun key_not_found(): u64 { abort 0 }
    /// Error when the maximum number of slices is reached.
    fun max_slice_amount_reached(): u64 { abort 0 }
    /// Error when trying to destroy a non-empty KeyedBigVector.
    fun not_empty(): u64 { abort 0 }

    // ======== Structs ========

    /// A data structure that allows for both indexed and keyed access to a large number of elements.
    public struct KeyedBigVector has key, store {
        /// The unique identifier of the KeyedBigVector object.
        id: UID,
        /// The type name of the keys.
        key_type: TypeName,
        /// The type name of the values.
        value_type: TypeName,
        /// The index of the latest slice.
        slice_idx: u16,
        /// The maximum size of each slice.
        slice_size: u32,
        /// The total number of elements in the KeyedBigVector.
        length: u64,
    }

    /// A slice of the KeyedBigVector, containing a vector of elements.
    public struct Slice<K: copy + drop + store, V: store> has store, drop {
        /// The index of the slice.
        idx: u16,
        /// The vector that stores the elements.
        vector: vector<Element<K, V>>,
    }

    /// An element in the KeyedBigVector, containing a key-value pair.
    public struct Element<K: copy + drop + store, V: store> has store, drop {
        /// The key of the element.
        key: K,
        /// The value of the element.
        value: V,
    }

    // ======== Functions ========

    /// Creates a new `KeyedBigVector`.
    /// The `slice_size` determines the maximum number of elements in each slice.
    public fun new<K: copy + drop + store, V: store>(slice_size: u32, ctx: &mut TxContext): KeyedBigVector {
        assert!(slice_size > 0 && slice_size <= CMaxSliceSize, invalid_slice_size());
        let mut id = object::new(ctx);
        dynamic_field::add(&mut id, SKeyIndexTable.to_string(), table::new<K, u64>(ctx));

        KeyedBigVector {
            id,
            key_type: type_name::with_defining_ids<K>(),
            value_type: type_name::with_defining_ids<V>(),
            slice_idx: 0,
            slice_size,
            length: 0,
        }
    }

    /// Returns the index of the latest slice in the KeyedBigVector.
    public fun slice_idx(kbv: &KeyedBigVector): u16 {
        kbv.slice_idx
    }

    /// Returns the maximum size of each slice in the KeyedBigVector.
    public fun slice_size(kbv: &KeyedBigVector): u32 {
        kbv.slice_size
    }

    /// Returns the total number of elements in the KeyedBigVector.
    public fun length(kbv: &KeyedBigVector): u64 {
        kbv.length
    }

    /// Returns `true` if the KeyedBigVector is empty.
    public fun is_empty(kbv: &KeyedBigVector): bool {
        kbv.length == 0
    }

    /// Returns `true` if there is a value associated with the key `key` in the KeyedBigVector.
    public fun contains<K: copy + drop + store>(kbv: &KeyedBigVector, key: K): bool {
        table::contains<K, u64>(dynamic_field::borrow(&kbv.id, SKeyIndexTable.to_string()), key)
    }

    /// Returns the index of the slice.
    public fun get_slice_idx<K: copy + drop + store, V: store>(slice: &Slice<K, V>): u16 {
        slice.idx
    }

    /// Returns the number of elements in the slice.
    public fun get_slice_length<K: copy + drop + store, V: store>(slice: &Slice<K, V>): u64 {
        slice.vector.length()
    }

    /// Pushes a new element to the end of the KeyedBigVector.
    /// Aborts if the key already exists or if the maximum number of slices is reached.
    public fun push_back<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, key: K, value: V) {
        assert!(!kbv.contains(key), duplicate_key());
        let element = Element { key, value };
        if (kbv.is_empty() || kbv.length() % (kbv.slice_size as u64) == 0) {
            kbv.slice_idx = (kbv.length() / (kbv.slice_size as u64) as u16);
            assert!(kbv.slice_idx < CMaxSliceAmount, max_slice_amount_reached());
            let new_slice = Slice {
                idx: kbv.slice_idx,
                vector: vector[element]
            };
            dynamic_field::add(&mut kbv.id, kbv.slice_idx, new_slice);
        }
        else {
            let slice = borrow_slice_mut_(&mut kbv.id, kbv.slice_idx);
            slice.vector.push_back(element);
        };
        table::add(dynamic_field::borrow_mut(&mut kbv.id, SKeyIndexTable.to_string()), key, kbv.length);
        kbv.length = kbv.length + 1;
    }

    /// Pops an element from the end of the KeyedBigVector and returns its key and value.
    /// Aborts if the KeyedBigVector is empty.
    public fun pop_back<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector): (K, V) {
        assert!(!kbv.is_empty(), index_out_of_bounds());

        let slice = borrow_slice_mut_(&mut kbv.id, kbv.slice_idx);
        let Element { key, value } = slice.vector.pop_back();
        kbv.trim_slice<K, V>();
        table::remove<K, u64>(dynamic_field::borrow_mut(&mut kbv.id, SKeyIndexTable.to_string()), key);
        kbv.length = kbv.length - 1;

        (key, value)
    }

    /// Borrows a slice from the KeyedBigVector at `slice_idx`.
    public fun borrow_slice<K: copy + drop + store, V: store>(kbv: &KeyedBigVector, slice_idx: u16): &Slice<K, V> {
        assert!(slice_idx <= kbv.slice_idx, index_out_of_bounds());

        borrow_slice_(&kbv.id, slice_idx)
    }
    fun borrow_slice_<K: copy + drop + store, V: store>(id: &UID, slice_idx: u16): &Slice<K, V> {
        dynamic_field::borrow(id, slice_idx)
    }

    /// Borrows a mutable slice from the KeyedBigVector at `slice_idx`.
    public fun borrow_slice_mut<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, slice_idx: u16): &mut Slice<K, V> {
        assert!(slice_idx <= kbv.slice_idx, index_out_of_bounds());

        borrow_slice_mut_(&mut kbv.id, slice_idx)
    }
    fun borrow_slice_mut_<K: copy + drop + store, V: store>(id: &mut UID, slice_idx: u16): &mut Slice<K, V> {
        dynamic_field::borrow_mut(id, slice_idx)
    }

    /// Borrows an element at index `i` from the KeyedBigVector.
    public fun borrow<K: copy + drop + store, V: store>(kbv: &KeyedBigVector, i: u64): (K, &V) {
        assert!(i < kbv.length, index_out_of_bounds());

        borrow_(kbv, i)
    }
    fun borrow_<K: copy + drop + store, V: store>(kbv: &KeyedBigVector, i: u64): (K, &V) {
        let slice = borrow_slice_(&kbv.id, (i / (kbv.slice_size as u64) as u16));
        let element = &slice.vector[i % (kbv.slice_size as u64)];

        (element.key, &element.value)
    }

    /// Borrows a mutable element at index `i` from the KeyedBigVector.
    public fun borrow_mut<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, i: u64): (K, &mut V) {
        assert!(i < kbv.length, index_out_of_bounds());

        borrow_mut_(kbv, i)
    }
    fun borrow_mut_<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, i: u64): (K, &mut V) {
        let slice = borrow_slice_mut_(&mut kbv.id, (i / (kbv.slice_size as u64) as u16));
        let element = &mut slice.vector[i % (kbv.slice_size as u64)];

        (element.key, &mut element.value)
    }

    /// Borrows an element by its key from the KeyedBigVector.
    #[syntax(index)]
    public fun borrow_by_key<K: copy + drop + store, V: store>(kbv: &KeyedBigVector, key: K): &V {
        assert!(kbv.contains(key), key_not_found());

        let i = *table::borrow<K, u64>(dynamic_field::borrow(&kbv.id, SKeyIndexTable.to_string()), key);
        let (_, v) = borrow_<K, V>(kbv, i);

        v
    }

    /// Borrows a mutable element by its key from the KeyedBigVector.
    #[syntax(index)]
    public fun borrow_by_key_mut<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, key: K): &mut V {
        assert!(kbv.contains(key), key_not_found());

        let i = *table::borrow<K, u64>(dynamic_field::borrow(&kbv.id, SKeyIndexTable.to_string()), key);
        let (_, v) = borrow_mut_<K, V>(kbv, i);

        v
    }

    /// Borrows an element at index `i` from a slice.
    public fun borrow_from_slice<K: copy + drop + store, V: store>(slice: &Slice<K, V>, i: u64): (K, &V) {
        assert!(i < slice.vector.length(), index_out_of_bounds());

        let element = &slice.vector[i];

        (element.key, &element.value)
    }

    /// Borrows a mutable element at index `i` from a slice.
    public fun borrow_from_slice_mut<K: copy + drop + store, V: store>(slice: &mut Slice<K, V>, i: u64): (K, &mut V) {
        assert!(i < slice.vector.length(), index_out_of_bounds());

        let element = &mut slice.vector[i];

        (element.key, &mut element.value)
    }

    /// Swaps the element at index `i` with the last element and removes it.
    public fun swap_remove<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, i: u64): (K, V) {
        assert!(i < kbv.length, index_out_of_bounds());

        swap_remove_(kbv, i)
    }
    fun swap_remove_<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, i: u64): (K, V) {
        let (key, value) = pop_back(kbv);
        if (i == kbv.length()) {
            (key, value)
        } else {
            table::add(dynamic_field::borrow_mut(&mut kbv.id, SKeyIndexTable.to_string()), key, i);
            let slice = borrow_slice_mut_(&mut kbv.id, (i / (kbv.slice_size as u64) as u16));
            slice.vector.push_back(Element { key, value });
            let Element { key, value } = slice.vector.swap_remove(i % (kbv.slice_size as u64));
            table::remove<K, u64>(dynamic_field::borrow_mut(&mut kbv.id, SKeyIndexTable.to_string()), key);
            (key, value)
        }
    }

    /// Swaps the element with the given key with the last element and removes it.
    public fun swap_remove_by_key<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector, key: K): V {
        assert!(kbv.contains(key), key_not_found());

        let i = *table::borrow<K, u64>(dynamic_field::borrow(&kbv.id, SKeyIndexTable.to_string()), key);
        let (_, v) = swap_remove_<K, V>(kbv, i);

        v
    }

    /// Destroys an empty KeyedBigVector.
    /// Aborts if the KeyedBigVector is not empty.
    public fun destroy_empty(kbv: KeyedBigVector) {
        let KeyedBigVector {
            id,
            key_type: _,
            value_type: _,
            slice_idx: _,
            slice_size: _,
            length,
        } = kbv;
        assert!(length == 0, not_empty());
        id.delete();
    }

    /// Destroys a KeyedBigVector.
    public fun drop(kbv: KeyedBigVector) {
        let KeyedBigVector {
            id,
            key_type: _,
            value_type: _,
            slice_idx: _,
            slice_size: _,
            length: _,
        } = kbv;
        id.delete();
    }

    /// Destroys a KeyedBigVector and its elements completely.
    public fun completely_drop<K: copy + drop + store, V: drop + store>(kbv: KeyedBigVector) {
        let KeyedBigVector {
            mut id,
            key_type: _,
            value_type: _,
            slice_idx,
            slice_size: _,
            length,
        } = kbv;
        if (length > 0) {
            (slice_idx + 1).do!(|i| {
                dynamic_field::remove<u16, Slice<K, V>>(&mut id, slice_idx - i);
            });
        };
        id.delete();
    }

    /// Removes an empty slice after an element has been removed from it.
    fun trim_slice<K: copy + drop + store, V: store>(kbv: &mut KeyedBigVector) {
        let slice = borrow_slice_(&kbv.id, kbv.slice_idx);
        if (slice.vector.is_empty<Element<K, V>>()) {
            let Slice {
                idx: _,
                vector: v,
            } = dynamic_field::remove(&mut kbv.id, kbv.slice_idx);
            v.destroy_empty<Element<K, V>>();
            if (kbv.slice_idx > 0) {
                kbv.slice_idx = kbv.slice_idx - 1;
            };
        };
    }

    /// A macro for iterating over the elements of a KeyedBigVector with immutable references.
    public macro fun do_ref<$K, $V>($kbv: &KeyedBigVector, $f: |$K, &$V|) {
        let kbv = $kbv;
        let length = kbv.length();
        if (length > 0) {
            let slice_size = (kbv.slice_size() as u64);
            let mut slice = kbv.borrow_slice(0);
            length.do!(|i| {
                let (key, value) = slice.borrow_from_slice(i % slice_size);
                $f(key, value);
                // jump to next slice
                if (i + 1 < length && (i + 1) % slice_size == 0) {
                    slice = kbv.borrow_slice(((i + 1) / slice_size) as u16);
                };
            });
        };
    }

    /// A macro for iterating over the elements of a KeyedBigVector with mutable references.
    public macro fun do_mut<$K, $V>($kbv: &mut KeyedBigVector, $f: |$K, &mut $V|) {
        let kbv = $kbv;
        let length = kbv.length();
        if (length > 0) {
            let slice_size = (kbv.slice_size() as u64);
            let mut slice = kbv.borrow_slice_mut(0);
            length.do!(|i| {
                let (key, value) = slice.borrow_from_slice_mut(i % slice_size);
                $f(key, value);
                // jump to next slice
                if (i + 1 < length && (i + 1) % slice_size == 0) {
                    slice = kbv.borrow_slice_mut(((i + 1) / slice_size) as u16);
                };
            });
        };
    }

    #[test, expected_failure]
    fun test_duplicate_key() {
        duplicate_key();
    }
    #[test, expected_failure]
    fun test_index_out_of_bounds() {
        index_out_of_bounds();
    }
    #[test, expected_failure]
    fun test_invalid_slice_size() {
        invalid_slice_size();
    }
    #[test, expected_failure]
    fun test_key_not_found() {
        key_not_found();
    }
    #[test, expected_failure]
    fun test_max_slice_amount_reached() {
        max_slice_amount_reached();
    }
    #[test, expected_failure]
    fun test_not_empty() {
        not_empty();
    }
}