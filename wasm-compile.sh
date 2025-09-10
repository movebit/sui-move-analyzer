export CC="$WASI_SDK_PATH/bin/clang --sysroot=$WASI_SDK_PATH/share/wasi-sysroot"
export AR="$WASI_SDK_PATH/bin/llvm-ar"
export RUSTFLAGS="-A warnings -C target-feature=+atomics,+bulk-memory,+mutable-globals -C panic=unwind -C link-arg=--max-memory=2147483648" && \
cargo build \
    -Zbuild-std=std,panic_abort \
    --target=wasm32-wasip1-threads \
    --release
cp -f /data/zhangxiao/move/sui-move-analyzer/target/wasm32-wasip1-threads/release/sui_move_analyzer2.wasm /data/zhangxiao/move/web-ide/move-web-ide-bak/webIDE/public