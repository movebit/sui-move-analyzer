// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import type { Context } from './context';
import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import * as childProcess from 'child_process';

// 创建一个输出通道用于调试日志
const outputChannel = vscode.window.createOutputChannel('Sui Move Analyzer Graph');



/**
 * A logger for the VS Code extension.
 *
 * Messages that are logged appear in an output channel created below that is dedicated to the
 * extension (or "client"), in the extension user's "Output View." This logger should be used for
 * messages related to VS Code and this extension, as opposed to messages regarding the language
 * server, which appear in a separate output channel.
 **/

class TraverseDirItem {
    path: string;

    is_file: boolean;

    constructor(path: string,
        is_file: boolean) {
        this.path = path;
        this.is_file = is_file;
    }
}


function workSpaceDir(): string | undefined {
    if (vscode.workspace.workspaceFolders !== undefined) {
        if (vscode.workspace.workspaceFolders[0] !== undefined) {
            const f = vscode.workspace.workspaceFolders[0].uri.fsPath;
            return f;
        }
    }
    return undefined;
}

async function serverVersion(context: Readonly<Context>): Promise<void> {
    const version = childProcess.spawnSync(
        context.configuration.serverPath,
        ['--version'],
        { encoding: 'utf8' },
    );
    if (version.stdout) {
        await vscode.window.showInformationMessage(version.stdout);
    } else if (version.error) {
        await vscode.window.showErrorMessage(
            `Could not execute sui-move-analyzer: ${version.error.message}.`,
        );
    } else {
        await vscode.window.showErrorMessage(
            `A problem occurred when executing '${context.configuration.serverPath}'.`,
        );
    }
}

function traverseDir(dir: any, call_back: (path: TraverseDirItem) => void): void {
    fs.readdirSync(dir).forEach(file => {
        const fullPath = path.join(dir, file);
        if (fs.lstatSync(fullPath).isDirectory()) {
            call_back(new TraverseDirItem(fullPath, false));
            traverseDir(fullPath, call_back);
        } else {
            call_back(new TraverseDirItem(fullPath, true));
        }
    });
}

function get_all_move_toml_dirs(): string[] {
    const working_dir = workSpaceDir();
    if (working_dir === undefined) {
        return [];
    }
    const ret: string[] = [];
    traverseDir(working_dir, (item) => {
        if (item.is_file && item.path.endsWith('Move.toml')) {
            ret.push(item.path);
        }
    });
    return ret;
}

class TerminalManager {
    all: Map<string, vscode.Terminal | undefined>;

    constructor() {
        this.all = new Map();
    }

    alloc(typ: string, new_fun: () => vscode.Terminal): vscode.Terminal {
        const x = this.all.get(typ);
        if (x === undefined || x.exitStatus !== undefined) {
            const x = new_fun();
            this.all.set(typ, x);
            return x;
        }
        return x;
    }
}


class WorkingDir {

    private dir: string | undefined;

    constructor() {
        const working_dir = workSpaceDir();
        if (working_dir === undefined) {
            this.dir = undefined;
        }
        const x = get_all_move_toml_dirs();
        if (x.length === 1) {
            this.dir = working_dir;
        }
        this.dir = undefined;
    }

    // Change the current working dir
    set_dir(Dir: string): void {
        this.dir = Dir;
    }

    // Get the current working dir, if is undefined, return ""
    get_dir(): string {
        if (this.dir !== undefined) {
            return this.dir;
        }
        return '';
    }

    async get_use_input_working_dir(): Promise<string | undefined> {
        return vscode.window.showQuickPick(get_all_move_toml_dirs(),
            {
            }).then((x): string | undefined => {
                if (x === undefined) {
                    return undefined;
                }
                this.dir = path.parse(x).dir;
                return this.dir;
            });
    }

    async get_working_dir(): Promise<string | undefined> {
        if (this.dir !== undefined) {
            return this.dir;
        }
        return this.get_use_input_working_dir();
    }

}
const Reg = {

    /** Regist all the command for sui framework for main.ts */
    regsui(context: Readonly<Context>): void {
        /**
         * An extension command that displays the version of the server that this extension
         * interfaces with.
         */
        const sui_working_dir = new WorkingDir();
        const terminalManager = new TerminalManager();
        const schemaTypes = ['ed25519', 'secp256k1', 'secp256r1'];
        const sui_move_toml_template = `[package]
        name = "my_first_package"
        version = "0.0.1"

        [dependencies]
        Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet" }

        [addresses]
        my_first_package =  "0x0"
        sui =  "0x2"
        `;
        const sui_module_file_template = `
        // Copyright (c) Mysten Labs, Inc.
        // SPDX-License-Identifier: Apache-2.0

        module my_first_package::my_module {
            // Part 1: imports
            use sui::object::{Self, UID};
            use sui::transfer;
            use sui::tx_context::{Self, TxContext};

            // Part 2: struct definitions
            struct Sword has key, store {
                id: UID,
                magic: u64,
                strength: u64,
            }

            struct Forge has key {
                id: UID,
                swords_created: u64,
            }

            // Part 3: module initializer to be executed when this module is published
            fun init(ctx: &mut TxContext) {
                let admin = Forge {
                    id: object::new(ctx),
                    swords_created: 0,
                };
                // transfer the forge object to the module/package publisher
                transfer::transfer(admin, tx_context::sender(ctx));
            }

            // Part 4: accessors required to read the struct attributes
            public fun magic(self: &Sword): u64 {
                self.magic
            }

            public fun strength(self: &Sword): u64 {
                self.strength
            }

            public fun swords_created(self: &Forge): u64 {
                self.swords_created
            }

            // Part 5: entry functions to create and transfer swords
            public entry fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address,
                                          ctx: &mut TxContext) {
                // create a sword
                let sword = Sword {
                    id: object::new(ctx),
                    magic: magic,
                    strength: strength,
                };
                // transfer the sword
                transfer::transfer(sword, recipient);
                forge.swords_created = forge.swords_created + 1;
            }
        }
        `;

        if (sui_working_dir.get_dir() !== '') {
            void vscode.window.showInformationMessage('sui working directory set to ' + sui_working_dir.get_dir());
        }

        // Register handlers for VS Code commands that the user explicitly issues.
        context.registerCommand('serverVersion', serverVersion);
        
        // Register graph commands - these will be implemented separately
        context.registerCommand('showStructDependencyGraph', () => {
            // Defer implementation to a separate function to avoid circular imports
            void showStructDependencyGraph(context);
        });
        
        context.registerCommand('showCallFlowGraph', () => {
            // Defer implementation to a separate function to avoid circular imports
            void showCallFlowGraph(context);
        });

        // Register test button
        context.registerCommand('test_ui', (_, ...args) => {
            const cwd = args[0] as string;
            const name = args[1] as string;
            const sui_test = terminalManager.alloc(cwd + 'test_ui', () => {
                return vscode.window.createTerminal({
                    cwd: cwd,
                    name: 'sui test',
                });
            });
            sui_test.show(true);
            sui_test.sendText('sui move test ' + name, true);
            sui_test.show(false);
        });

        context.registerCommand('create_project', async () => {

            const dir = await vscode.window.showSaveDialog({
                // There is a long term issue about parse()
                // use "." instead of working dir, detail in https://github.com/microsoft/vscode/issues/173687
                defaultUri: vscode.Uri.parse('.'),
            });

            if (dir === undefined) {
                void vscode.window.showErrorMessage('Please input a directory');
                return;
            }
            const dir2 = dir.fsPath;
            fs.mkdirSync(dir2);
            const project_name = path.parse(dir2).base;
            const replace_name = 'my_first_package';
            fs.writeFileSync(dir2 + '/Move.toml',
                sui_move_toml_template.toString().replaceAll(replace_name, project_name));
            fs.mkdirSync(dir2 + '/sources');
            fs.writeFileSync(dir2 + '/sources/my_module.move',
                sui_module_file_template.replaceAll(replace_name, project_name));
        });
        context.registerCommand('move.new', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const name = await vscode.window.showInputBox({
                title: 'New a project',
                placeHolder: 'Type you project name.',
            });
            if (name === undefined) {
                return;
            }
            const t = terminalManager.alloc('move.new', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui move new',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui move new ' + name, true);
        });
        context.registerCommand('move.build', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('move.build', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui move build',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui move build', true);
        });
        context.registerCommand('move.coverage', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('move.coverage', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui move coverage',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui move test --coverage', true);
            t.sendText('sui move coverage summary', true);
        });
        context.registerCommand('move.test', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('move.test', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui move test',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true); t.sendText('cd ' + working_dir, true);
            t.sendText('sui move test', true);
        });
        context.registerCommand('move.prove', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('move.prove', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui move prove',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui move prove', true);
        });
        context.registerCommand('client.active.address', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.active.address', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client active address',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client active-address', true);
        });
        context.registerCommand('client.active.env', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.active.env', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client active env',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client active-env', true);
        });
        context.registerCommand('client.addresses', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.addresses', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client addresses',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client addresses', true);
        });
        context.registerCommand('client.envs', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.envs', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client envs',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client envs', true);
        });
        context.registerCommand('client.gas', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.gas', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client gas',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client gas', true);
        });
        context.registerCommand('client.object', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const objectID = await vscode.window.showInputBox({
                placeHolder: 'Type you object ID.',
            });
            if (objectID === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.object', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client object',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client object ' + objectID, true);
        });
        context.registerCommand('client.objects', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.objects', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client objects',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client objects', true);
        });
        context.registerCommand('client.publish', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const budget = await vscode.window.showInputBox({
                placeHolder: 'Type you Gas Budget.',
            });
            if (budget === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.publish', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client publish',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client publish --gas-budget ' + budget, true);
        });
        context.registerCommand('client.new.address', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const schema = await vscode.window.showQuickPick(schemaTypes, {
                canPickMany: false, placeHolder: 'Select you schema.',
            });
            if (schema === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.new.address', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui client new address',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui client new-address ' + schema, true);
        });
        context.registerCommand('keytool.generate', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const schema = await vscode.window.showQuickPick(schemaTypes, {
                canPickMany: false, placeHolder: 'Select you schema.',
            });
            if (schema === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.keytool.generate', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui keytool generate',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui keytool generate ' + schema, true);
        });
        context.registerCommand('keytool.import', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const m = await vscode.window.showInputBox({
                placeHolder: 'Type your mnemonic phrase.',
            });
            if (m === undefined) {
                return;
            }
            const schema = await vscode.window.showQuickPick(schemaTypes, {
                canPickMany: false, placeHolder: 'Select you schema.',
            });
            if (schema === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.keytool.import', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui keytool import',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui keytool import ' + m + ' ' + schema, true);
        });
        context.registerCommand('keytool.list', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.keytool.list', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui keytool list',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui keytool list ', true);
        });
        context.registerCommand('keytool.load.keypair', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const file = await vscode.window.showOpenDialog({
                canSelectFiles: false,
            });
            if (file === undefined) {
                return;
            }
            if (file.length === 0) {
                return;
            }
            if (file[0] !== undefined) {
                const t = terminalManager.alloc('client.keytool.load.keypair', (): vscode.Terminal => {
                    return vscode.window.createTerminal({
                        name: 'sui keytool load keypair',
                    });
                });
                t.show(true);
                t.sendText('cd ' + working_dir, true);
                t.sendText('sui keytool load-keypair ' + file[0].fsPath, true);
            }
        });
        context.registerCommand('keytool.show', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const file = await vscode.window.showOpenDialog({
                canSelectFiles: false,
            });
            if (file === undefined) {
                return;
            }
            if (file.length === 0) {
                return;
            }
            if (file[0] !== undefined) {
                const t = terminalManager.alloc('client.keytool.show', (): vscode.Terminal => {
                    return vscode.window.createTerminal({
                        name: 'sui keytool show',
                    });
                });
                t.show(true);
                t.sendText('cd ' + working_dir, true);
                t.sendText('sui keytool show ' + file[0].fsPath, true);
            }
        });
        context.registerCommand('keytool.unpack', async () => {
            const working_dir = await sui_working_dir.get_working_dir();
            if (working_dir === undefined) {
                return;
            }
            const str = await vscode.window.showInputBox({
                placeHolder: 'Type your ????',
            });
            if (str === undefined) {
                return;
            }
            const t = terminalManager.alloc('client.keytool.unpack', (): vscode.Terminal => {
                return vscode.window.createTerminal({
                    name: 'sui keytool unpack',
                });
            });
            t.show(true);
            t.sendText('cd ' + working_dir, true);
            t.sendText('sui keytool unpack \'' + str + '\'', true);
        });
        context.registerCommand('reset.working.space', async () => {
            const new_ = await sui_working_dir.get_use_input_working_dir();
            if (new_ === undefined) {
                return;
            }
            sui_working_dir.set_dir(new_);
            void vscode.window.showInformationMessage('sui working directory set to ' + new_);
        });
        context.registerCommand('runLinter', (_, ...args) => {
            interface FsPath {
                fsPath: string;
            }
            if (args.length === 0) {
                console.log("runlinter args = 0");
                return;
            }
            const fsPath = (args[0] as FsPath).fsPath;
            const client = context.getClient();
            if (client === undefined) {
                return;
            }
            interface Result {
                result_msg: string;
            }
            // const working_dir = sui_working_dir.get_use_input_working_dir();
            // if (working_dir === undefined) {
            //     return;
            // }
            client.sendRequest<Result>('runLinter', { 'fpath': fsPath }).then(
                (result) => {
                    console.log("run linter result.result_msg = ", result.result_msg);
                    // void vscode.window.showErrorMessage('run linter result: ' + result.result_msg);
                },
            ).catch((err) => {
                void vscode.window.showErrorMessage('run linter failed: ' + (err as string));
            });
        });
        context.registerCommand('move.generate.spec.file', (_, ...args) => {
            interface FsPath {
                fsPath: string;
            }
            if (args.length === 0) {
                return;
            }
            const fsPath = (args[0] as FsPath).fsPath;
            if (fsPath.endsWith('.spec.move')) {
                void vscode.window.showErrorMessage('This is already a spec file');
                return;
            }
            const client = context.getClient();
            if (client === undefined) {
                return;
            }
            interface Result {
                fpath: string;
            }
            client.sendRequest<Result>('move/generate/spec/file', { 'fpath': fsPath }).then(
                (result) => {
                    void vscode.workspace.openTextDocument(result.fpath).then((a) => {
                        void vscode.window.showTextDocument(a);
                    });
                },
            ).catch((err) => {
                void vscode.window.showErrorMessage('generate failed: ' + (err as string));
            });
        });
        context.registerCommand('move.generate.spec.sel', (_, ...args) => {
            interface FsPath {
                fsPath: string;
            }
            if (args.length === 0) {
                return;
            }
            if (vscode.window.activeTextEditor === undefined) {
                return;
            }
            const line = vscode.window.activeTextEditor.selection.active.line;
            const col = vscode.window.activeTextEditor.selection.active.character;
            const fsPath = (args[0] as FsPath).fsPath;
            if (fsPath.endsWith('.spec.move')) {
                void vscode.window.showErrorMessage('This is already a spec file');
                return;
            }
            const client = context.getClient();
            if (client === undefined) {
                return;
            }
            interface Result {
                content: string;
                line: number;
                col: number;
            }

            client.sendRequest<Result>('move/generate/spec/sel', { 'fpath': fsPath, line: line, col: col }).then(
                (result) => {
                    vscode.window.activeTextEditor?.edit((e) => {
                        e.insert(new vscode.Position(result.line, result.col), result.content);
                    });
                },
            ).catch((err) => {
                void vscode.window.showErrorMessage('generate failed: ' + (err as string));
            });
        });
    },

};

// Helper functions for graph display
async function showStructDependencyGraph(context: Readonly<Context>) {
    outputChannel.appendLine('[Graph Debug] Starting showStructDependencyGraph function...');
    
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        const errorMsg = 'No active editor found';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }

    outputChannel.appendLine(`[Graph Debug] Active document: ${editor.document.fileName}`);
    
    const document = editor.document;
    if (document.languageId !== 'move') {
        const errorMsg = 'Current file is not a Move file';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }
    
    outputChannel.appendLine(`[Graph Debug] Document language ID: ${document.languageId}`);

    // Request graph data from the language server
    const client = context.getClient();
    if (!client) {
        const errorMsg = 'No language server client available';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }
    
    outputChannel.appendLine('[Graph Debug] Language server client found, sending request...');

    try {
        outputChannel.appendLine('[Graph Debug] Sending move/struct_dependency/graph request...');
        
        const graphData = await client.sendRequest<any>(
            'move/struct_dependency/graph',
            {
                textDocument: {
                    uri: document.uri.toString()
                },
                graphType: 'struct_dependency',
                format: 'dot'  // Request DOT format for better visualization
            }
        );
        
        outputChannel.appendLine(`[Graph Debug] Received response from server: ${JSON.stringify(graphData, null, 2)}`);

        if (graphData && graphData.graph_data) {
            outputChannel.appendLine('[Graph Debug] Valid graph data received, creating webview panel...');
            
            // Create and show a webview panel to display the graph
            const panel = vscode.window.createWebviewPanel(
                'structDependencyGraph',
                'Struct Dependency Graph',
                vscode.ViewColumn.One,
                {
                    enableScripts: true,
                    retainContextWhenHidden: true
                }
            );

            // Generate HTML for the graph visualization
            panel.webview.html = getGraphHtml(JSON.parse(graphData.graph_data), 'Struct Dependency', context);
            
            outputChannel.appendLine('[Graph Debug] Webview panel created and displayed successfully');
        } else {
            const errorMsg = 'No struct dependency graph data received';
            outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
            outputChannel.appendLine(`[Graph Debug] Raw response: ${JSON.stringify(graphData)}`);
            vscode.window.showErrorMessage(errorMsg);
        }
    } catch (error) {
        const errorMsg = `Failed to get struct dependency graph: ${error}`;
        outputChannel.appendLine(`[Graph Debug] Exception caught: ${errorMsg}`);
        outputChannel.appendLine(`[Graph Debug] Error stack: ${(error as Error).stack || 'No stack trace'}`);
        vscode.window.showErrorMessage(errorMsg);
    }
}

async function showCallFlowGraph(context: Readonly<Context>) {
    outputChannel.appendLine('[Graph Debug] Starting showCallFlowGraph function...');
    
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        const errorMsg = 'No active editor found';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }

    outputChannel.appendLine(`[Graph Debug] Active document: ${editor.document.fileName}`);
    
    const document = editor.document;
    if (document.languageId !== 'move') {
        const errorMsg = 'Current file is not a Move file';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }
    
    outputChannel.appendLine(`[Graph Debug] Document language ID: ${document.languageId}`);

    // Request graph data from the language server
    const client = context.getClient();
    if (!client) {
        const errorMsg = 'No language server client available';
        outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
        vscode.window.showErrorMessage(errorMsg);
        return;
    }
    
    outputChannel.appendLine('[Graph Debug] Language server client found, sending request...');

    try {
        outputChannel.appendLine('[Graph Debug] Sending move/call_flow/graph request...');
        
        const graphData = await client.sendRequest<any>(
            'move/call_flow/graph',
            {
                textDocument: {
                    uri: document.uri.toString()
                },
                graphType: 'call_flow',
                format: 'dot'  // Request DOT format for better visualization
            }
        );
        
        outputChannel.appendLine(`[Graph Debug] Received response from server: ${JSON.stringify(graphData, null, 2)}`);

        if (graphData && graphData.graph_data) {
            outputChannel.appendLine('[Graph Debug] Valid graph data received, creating webview panel...');
            
            // Create and show a webview panel to display the graph
            const panel = vscode.window.createWebviewPanel(
                'callFlowGraph',
                'Function Call Flow Graph',
                vscode.ViewColumn.One,
                {
                    enableScripts: true,
                    retainContextWhenHidden: true
                }
            );

            // Generate HTML for the graph visualization
            panel.webview.html = getGraphHtml(JSON.parse(graphData.graph_data), 'Function Call Flow', context);
            
            outputChannel.appendLine('[Graph Debug] Webview panel created and displayed successfully');
        } else {
            const errorMsg = 'No call flow graph data received';
            outputChannel.appendLine(`[Graph Debug] Error: ${errorMsg}`);
            outputChannel.appendLine(`[Graph Debug] Raw response: ${JSON.stringify(graphData)}`);
            vscode.window.showErrorMessage(errorMsg);
        }
    } catch (error) {
        const errorMsg = `Failed to get call flow graph: ${error}`;
        outputChannel.appendLine(`[Graph Debug] Exception caught: ${errorMsg}`);
        outputChannel.appendLine(`[Graph Debug] Error stack: ${(error as Error).stack || 'No stack trace'}`);
        vscode.window.showErrorMessage(errorMsg);
    }
}

function getGraphHtml(graphData: any, title: string, _context: Readonly<Context>): string {
    outputChannel.appendLine(`[Graph Debug] Rendering high-quality graph HTML for ${title}`);
    
    // 1. 数据预处理
    let parsedData = graphData;
    if (typeof graphData === 'string') {
        try {
            parsedData = JSON.parse(graphData);
        } catch (e) {
            outputChannel.appendLine('[Graph Debug] Parse Error: ' + e);
            return `<h1>Error parsing graph data</h1>`;
        }
    }

    // 2. 将数据转换为 Cytoscape 元素格式
    const nodes = parsedData.nodes.map((node: any) => ({
        group: 'nodes',
        data: {
            id: node.id,
            label: node.label || node.id,
            module: node.module,
            address: node.address,
            // 增加父节点 ID，用于按 Module 分组（Compound Nodes）
            parent: node.module 
        }
    }));

    // 创建 Module 父节点，实现“容器”效果
    const modules = [...new Set(parsedData.nodes.map((n: any) => n.module))].map(mod => ({
        group: 'nodes',
        data: { id: mod, label: `Module: ${mod}` }
    }));

    const edges = parsedData.edges.map((edge: any) => ({
        group: 'edges',
        data: {
            id: `${edge.from}-${edge.to}`,
            source: edge.from,
            target: edge.to,
            label: edge.label || ''
        }
    }));

    const allElements = [...modules, ...nodes, ...edges];

    // 3. 返回完整的 HTML
    return `
    <!DOCTYPE html>
    <html>
    <head>
        <meta charset="UTF-8">
        <style>
            body { margin: 0; padding: 0; background-color: #1e1e1e; color: white; font-family: sans-serif; overflow: hidden; }
            #cy { width: 100vw; height: 100vh; display: block; }
            .controls { position: absolute; top: 15px; left: 15px; z-index: 10; display: flex; gap: 8px; }
            button { 
                background: #333; color: white; border: 1px solid #555; padding: 5px 12px; 
                cursor: pointer; font-size: 12px; border-radius: 3px; 
            }
            button:hover { background: #444; }
            .info-panel {
                position: absolute; bottom: 15px; right: 15px; background: rgba(30,30,30,0.8);
                padding: 10px; border: 1px solid #444; font-size: 11px; pointer-events: none;
            }
        </style>
        <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.26.0/cytoscape.min.js"></script>
        <script src="https://unpkg.com/dagre@0.7.4/dist/dagre.js"></script>
        <script src="https://cdn.jsdelivr.net/npm/cytoscape-dagre@2.5.0/cytoscape-dagre.min.js"></script>
    </head>
    <body>
        <div class="controls">
            <button onclick="window.runLayout('dagre')">Hierarchical (Dagre)</button>
            <button onclick="window.runLayout('cose')">Force Directed</button>
            <button onclick="window.cy.fit()">Fit All</button>
        </div>
        <div id="cy"></div>
        <div class="info-panel">
            <b>Sui Move Struct Graph</b><br/>
            Nodes: ${nodes.length} | Edges: ${edges.length}
        </div>
        <script>
            // 注册布局插件
            if (typeof cytoscapeDagre !== 'undefined') {
                cytoscape.use(cytoscapeDagre);
            }

            const cy = cytoscape({
                container: document.getElementById('cy'),
                elements: ${JSON.stringify(allElements)},
                style: [
                    {
                        selector: 'node',
                        style: {
                            'background-color': '#007acc',
                            'label': 'data(label)',
                            'color': '#fff',
                            'text-valign': 'center',
                            'font-size': '12px',
                            'width': 'label',
                            'padding': '10px',
                            'shape': 'round-rectangle'
                        }
                    },
                    {
                        selector: 'node:parent', // Module 容器样式
                        style: {
                            'background-opacity': 0.1,
                            'background-color': '#fff',
                            'label': 'data(label)',
                            'text-valign': 'top',
                            'text-halign': 'center',
                            'font-weight': 'bold',
                            'border-width': 1,
                            'border-color': '#555',
                            'color': '#aaa'
                        }
                    },
                    {
                        selector: 'edge',
                        style: {
                            'width': 2,
                            'line-color': '#666',
                            'target-arrow-color': '#666',
                            'target-arrow-shape': 'triangle',
                            'curve-style': 'bezier',
                            'label': 'data(label)',
                            'font-size': '10px',
                            'color': '#999',
                            'text-background-opacity': 1,
                            'text-background-color': '#1e1e1e',
                            'edge-text-rotation': 'autorotate'
                        }
                    },
                    {
                        selector: ':selected',
                        style: {
                            'background-color': '#ffcc00',
                            'line-color': '#ffcc00',
                            'target-arrow-color': '#ffcc00',
                            'color': '#000'
                        }
                    }
                ],
                layout: { name: 'dagre', rankDir: 'LR', nodeSep: 50 }
            });

            window.cy = cy;
            window.runLayout = (name) => {
                cy.layout({ name: name, animate: true, padding: 50 }).run();
            };
        </script>
    </body>
    </html>
    `;
}


export { Reg, WorkingDir };
