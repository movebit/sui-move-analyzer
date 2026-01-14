# sui-move-analyzer
**Table of Contents**
* [Introduction](#Introduction)
* [Features](#Features)
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

## Support <span id="Support">

1.If you find any issues, please report a GitHub issue to the [issue](https://github.com/movebit/sui-move-analyzer/issues) repository to get help.

2.Welcome to the developer discussion group as well: [MoveAnalyzer](https://t.me/moveanalyzer). 
