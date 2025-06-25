# wasm-pack build --target web --out-dir lsp-pkg -- . -Z build-std=std,panic_abort

cargo build -Zbuild-std=std,panic_abort --target=wasm32-wasip1-threads --release 
cp -f /data/zhangxiao/move/sui-move-analyzer/target/wasm32-wasip1-threads/release/sui_move_analyzer2.wasm /data/zhangxiao/move/web-ide/move-web-ide-bak/webIDE/public