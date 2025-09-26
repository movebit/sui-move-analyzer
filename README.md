# sui-move-analyzer
**Table of Contents**
* [Introduction](#Introduction)
* [Features](#Features)
* [Installation](#Installation)
* [Support](#Support)

## Introduction <span id="Introduction">
The **sui-move-analyzer** is a Visual Studio Code plugin for **Sui Move** language developed by [MoveBit](https://movebit.xyz). Although this is an alpha release, it has many useful features, such as **highlight, autocomplete, go to definition/references**, and so on.

## Features <span id="Features">

Here are some of the features of the sui-move-analyzer Visual Studio Code extension. To see them, open a
Move source file (a file with a `.move` file extension) and:

- See Move keywords and types highlighted in appropriate colors.
- As you type, Move keywords will appear as completion suggestions.
- If the opened Move source file is located within a buildable project (a `Move.toml` file can be
  found in one of its parent directories), the following advanced features will also be available:
  - compiler diagnostics
  - sui commands line tool(you need install Sui Client CLI locally)
  - sui project template
  - go to definition
  - go to references
  - type on hover
  - inlay hints
  - linter for move file
  - ...
Got it 👍 I’ll rewrite the **Installation** section in English, simplify it to reflect the new flow (just install from Marketplace), add the optional build instructions, and remove all the PATH-related steps.

## Installation <span id="Installation">

**Note**:

1. If you already have *move-analyzer* or *aptos-move-analyzer* installed, please disable them before installing **sui-move-analyzer** to avoid conflicts.
2. You need to install [Sui CLI](https://docs.sui.io/references/cli) first, otherwise some features will not work.

### Recommended: Install from Marketplace

The **sui-move-analyzer** is now fully integrated into the VSCode extension.
Simply install it from the **VSCode Marketplace**—no additional setup is required.

Steps:

1. Open VSCode (version 1.55.2 or later).
2. Open the Command Palette (`⇧⌘P` on macOS, or *View > Command Palette...*).
3. Select **Extensions: Install Extensions**.
4. Search for **sui-move-analyzer** in the Marketplace and click **Install**.
5. Open any `.move` file and start coding with highlighting, autocomplete, diagnostics, and more.

After installation, restart VSCode to ensure the extension loads properly.

### Optional: Build from Source

If you prefer to build the language server yourself:

```bash
git clone https://github.com/movebit/sui-move-analyzer.git
cd sui-move-analyzer
cargo build --release
```

The binary will be available at:

```
target/release/sui-move-analyzer
```



### Troubleshooting
Please note: If you don't see the version number, you can refer to the troubleshooting section."

#### [1] cannot find the `sui-move-analyzer` program
##### 1) windows
If you are installing this extension on a Windows system and have followed the steps in Section 1.A by running the windows-installer.msi, but executing `sui-move-analyzer --version` in the command line doesn't find the `sui-move-analyzer` program, the issue may be that VSCode cannot locate the configured environment variables. You can try the following:

   1. Restart VSCode and install the `sui-move-analyzer` VSCode extension.
   2. In the Windows system settings, find the user environment variable `PATH`. Look for an entry ending with `MoveBit\sui-move-analyzer\`, and copy it.
   3. Open the extension settings for `sui-move-analyzer` in the VSCode extension store. In the `sui-move-analyzer > server:path` entry, add the path ending with `MoveBit\sui-move-analyzer\` before `sui-move-analyzer`. The final result may look like: `C:\Users\YourUserName\AppData\Local\Apps\MoveBit\sui-move-analyzer\sui-move-analyzer.exe`
   4. Restart a terminal and try running `sui-move-analyzer --version` in the command line again.

##### 2) mac & linux
If you see an error message *language server executable `sui-move-analyzer` could not be found* in the
bottom-right of your Visual Studio Code screen when opening a Move file, it means that the
`sui-move-analyzer` executable could not be found in your `PATH`. You may try the following:

1. Confirm that invoking `sui-move-analyzer --version` in a command line terminal prints out
   `sui-move-analyzer version number`. If it doesn't, then retry the instructions in **[step 1]**. If it
   does successfully print this output, try closing and re-opening the Visual Studio Code
   application, as it may not have picked up the update to your `PATH`.
2. If you installed the `sui-move-analyzer` executable to a different location that is outside of your
   `PATH`, then you may have the extension look at this location by using the the Visual Studio Code
   settings (`⌘,` on macOS, or use the menu item *Code > Preferences > Settings*). Search for the
   `sui-move-analyzer.server.path` setting, and set it to the location of the `sui-move-analyzer` language
   server you installed.
3. If you're using it in MacOS, you may meet the error `Macos cannot verify if this app contains malicious software`, you need to add support for `sui-move-analyzer` in the system settings Program Trust.


#### [2] analyzer not work
##### A. Need Move.toml
Open a Move source file (a file with a .move file extension) and if the opened Move source file is located within a buildable project (a Move.toml file can be found in one of its parent directories), the following advanced features will be available:

  - compiler diagnostics
  - go to definition
  - go to references
  - type on hover
  - autocomplete
  - outline view
  - ...

Therefore, the Move.toml file must be found in the project directory for the plug-in's functionality to take effect.

In addition, if you have already opened the move project before, the installed plug-in will not take effect in time. You need to reopen the vscode window and open the move project code again before the plug-in is activated. 

##### B. Need Build Project with Move.toml
When you first open a project, there will be some **dependencies** (configured in Move.toml) that need to be downloaded, so you need to run the `sui move build` command first to `build` the project. During the build process, the **dependencies** will be downloaded. Once all the **dependencies** for the project have been downloaded, sui-move-analyzer can properly `parse` the **dependencies** and project source code.


#### [3] build failed with steps in Section 1.B
If `cargo install --git http://github.com/movebit/sui-move-analyzer --branch master sui-move-analyzer` run failed, and meet the 
error info as follows:
```
error: failed to run custom build command for librocksdb-sys...

--- stderr
thread 'main' panicked at 'Unable to find libclang: "couldn't find any valid shared libraries matching: 
['clang.dll', 'libclang.dll']..."'
```

It's because it relies on `MystenLabs/sui_move_build` library, which requires an LLVM environment. You can refer to [llvm-project](https://github.com/llvm/llvm-project) go and install llvm.


## Support <span id="Support">

1.If you find any issues, please report a GitHub issue to the [issue](https://github.com/movebit/sui-move-analyzer/issues) repository to get help.

2.Welcome to the developer discussion group as well: [MoveAnalyzer](https://t.me/moveanalyzer). 
