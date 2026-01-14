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
                graphType: 'struct_dependency'
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
                graphType: 'call_flow'
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
    outputChannel.appendLine(`[Graph Debug] Rendering graph HTML for ${title}`);
    outputChannel.appendLine(`[Graph Debug] Graph data nodes count: ${graphData.nodes?.length || 0}`);
    outputChannel.appendLine(`[Graph Debug] Graph data edges count: ${graphData.edges?.length || 0}`);
    
    // Return HTML that uses Vis.js for graph visualization
    return `
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>${title}</title>
            <style>
                body {
                    margin: 0;
                    padding: 0;
                    height: 100vh;
                    overflow: hidden;
                    background-color: #fff;
                }
                #graph-container {
                    width: 100%;
                    height: 100%;
                    border: 1px solid #ccc;
                }
                .controls {
                    position: absolute;
                    top: 10px;
                    right: 10px;
                    z-index: 1000;
                    background: white;
                    padding: 10px;
                    border-radius: 4px;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
                }
            </style>
        </head>
        <body>
            <div id="graph-container"></div>
            <div class="controls">
                <button onclick="fitToScreen()">Fit to Screen</button>
                <button onclick="centerView()">Center</button>
            </div>
            <script>
                // Embed Vis.js library code directly
                ${getVisJsLibrary()}
                
                // Add console logging for webview
                console.log('Graph data loaded:', ${JSON.stringify(graphData)});
                
                const graphContainer = document.getElementById('graph-container');
                let network = null;

                function renderGraph() {
                    console.log('Starting graph rendering...');
                    // Prepare the data for Vis Network
                    const nodes = new vis.DataSet(${JSON.stringify(graphData.nodes)});
                    const edges = new vis.DataSet(${JSON.stringify(graphData.edges)});

                    const container = document.getElementById('graph-container');
                    const graphData = {
                        nodes: nodes,
                        edges: edges
                    };

                    console.log('Nodes:', nodes.get());
                    console.log('Edges:', edges.get());

                    const options = {
                        nodes: {
                            shape: 'box',
                            font: {
                                size: 14,
                                face: 'Arial'
                            },
                            color: {
                                background: '#e0f7fa',
                                border: '#00838f',
                                highlight: {
                                    background: '#b2ebf2',
                                    border: '#006064'
                                }
                            },
                            borderWidth: 2
                        },
                        edges: {
                            width: 2,
                            color: {
                                color: '#00838f',
                                highlight: '#006064'
                            },
                            smooth: {
                                type: 'curvedCW',
                                roundness: 0.2
                            },
                            font: {
                                size: 12,
                                align: 'middle'
                            }
                        },
                        physics: {
                            enabled: true,
                            stabilization: { iterations: 100 }
                        },
                        interaction: {
                            tooltipDelay: 200,
                            hideEdgesOnDrag: false
                        }
                    };

                    network = new vis.Network(container, graphData, options);
                    console.log('Network rendered successfully');
                }

                function fitToScreen() {
                    if (network) {
                        network.fit();
                        console.log('Fitting to screen');
                    }
                }

                function centerView() {
                    if (network) {
                        network.moveTo({position: {x: 0, y: 0}});
                        console.log('Centering view');
                    }
                }

                // Render the graph when the page loads
                window.onload = renderGraph;
            </script>
        </body>
        </html>
    `;
}

function getVisJsLibrary(): string {
    // Return a minimal but functional Vis.js implementation
    // In a production environment, we would load this from a CDN
    return `
        // Minimal Vis.js implementation for demonstration
        var vis = (function() {
            function DataSet(data) {
                this._data = Array.isArray(data) ? data : [];
                this.get = function() { 
                    return this._data; 
                };
                this.getIds = function() {
                    return this._data.map(item => item.id);
                };
            }
            
            function Network(container, data, options) {
                this.container = container;
                this.data = data;
                this.options = options || {};
                
                // Simple visualization logic
                this.fit = function() { 
                    console.log("Fit to screen called");
                };
                
                this.moveTo = function(pos) { 
                    console.log("Move to called with position:", pos);
                };
                
                // Create a simple SVG representation of the graph
                this.renderSimpleGraph = function() {
                    const svgNS = "http://www.w3.org/2000/svg";
                    const svg = document.createElementNS(svgNS, "svg");
                    svg.setAttribute("width", "100%");
                    svg.setAttribute("height", "100%");
                    svg.style.backgroundColor = "#f0f0f0";
                    
                    // Draw edges
                    if (data.edges && data.edges._data) {
                        data.edges._data.forEach((edge, idx) => {
                            const line = document.createElementNS(svgNS, "line");
                            line.setAttribute("x1", (50 + (idx * 40) % 200) + "%");
                            line.setAttribute("y1", (20 + (idx * 20) % 60) + "%");
                            line.setAttribute("x2", (70 + ((idx + 1) * 40) % 200) + "%");
                            line.setAttribute("y2", (40 + ((idx + 1) * 20) % 60) + "%");
                            line.setAttribute("stroke", "#00838f");
                            line.setAttribute("stroke-width", "2");
                            line.setAttribute("marker-end", "url(#arrow)");
                            
                            svg.appendChild(line);
                        });
                    }
                    
                    // Draw nodes
                    if (data.nodes && data.nodes._data) {
                        data.nodes._data.forEach((node, idx) => {
                            const circle = document.createElementNS(svgNS, "circle");
                            circle.setAttribute("cx", (10 + (idx * 30) % 80) + "%");
                            circle.setAttribute("cy", (10 + (idx * 20) % 80) + "%");
                            circle.setAttribute("r", "15");
                            circle.setAttribute("fill", "#e0f7fa");
                            circle.setAttribute("stroke", "#00838f");
                            circle.setAttribute("stroke-width", "2");
                            
                            const text = document.createElementNS(svgNS, "text");
                            text.setAttribute("x", (10 + (idx * 30) % 80) + "%");
                            text.setAttribute("y", (10 + (idx * 20) % 80) + "%");
                            text.setAttribute("text-anchor", "middle");
                            text.setAttribute("dy", "0.3em");
                            text.setAttribute("font-size", "12");
                            text.textContent = node.label || node.id;
                            
                            svg.appendChild(circle);
                            svg.appendChild(text);
                        });
                    }
                    
                    // Add arrow marker for edges
                    const defs = document.createElementNS(svgNS, "defs");
                    const marker = document.createElementNS(svgNS, "marker");
                    marker.setAttribute("id", "arrow");
                    marker.setAttribute("viewBox", "0 0 10 10");
                    marker.setAttribute("refX", "10");
                    marker.setAttribute("refY", "5");
                    marker.setAttribute("markerWidth", "6");
                    marker.setAttribute("markerHeight", "6");
                    marker.setAttribute("orient", "auto-start-reverse");
                    
                    const path = document.createElementNS(svgNS, "path");
                    path.setAttribute("d", "M 0 0 L 10 5 L 0 10 z");
                    path.setAttribute("fill", "#00838f");
                    
                    marker.appendChild(path);
                    defs.appendChild(marker);
                    svg.appendChild(defs);
                    
                    // Clear container and add SVG
                    while (container.firstChild) {
                        container.removeChild(container.firstChild);
                    }
                    container.appendChild(svg);
                };
                
                // Initialize the graph
                setTimeout(() => {
                    this.renderSimpleGraph();
                }, 100);
            }
            
            return {
                DataSet: DataSet,
                Network: Network
            };
        })();
    `;
}

export { Reg, WorkingDir };
