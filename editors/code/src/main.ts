// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Configuration } from './configuration';
import { Context } from './context';
import { Extension } from './extension';
import { log } from './log';
import { Reg } from './reg';
import * as commands from './commands';
import * as vscode from 'vscode';
// import * as os from 'os';
// import * as path from "path";
// import * as fs from "fs";

// let analyzerLspPath: string | undefined;
/**
 * The entry point to this VS Code extension.
 *
 * As per [the VS Code documentation on activation
 * events](https://code.visualstudio.com/api/references/activation-events), "an extension must
 * export an `activate()` function from its main module and it will be invoked only once by
 * VS Code when any of the specified activation events [are] emitted."
 *
 * Activation events for this extension are listed in its `package.json` file, under the key
 * `"activationEvents"`.
 *
 * In order to achieve synchronous activation, mark the function as an asynchronous function,
 * so that you can wait for the activation to complete by await
 */


export async function activate(
  extensionContext: Readonly<vscode.ExtensionContext>,
): Promise<void> {
  const extension = new Extension();
  log.info(`${extension.identifier} version ${extension.version}`);

  const configuration = new Configuration(extensionContext.extensionUri);
  log.info(`configuration: ${configuration.toString()}`);

  // await maybeDownloadLspServer();
  // log.info('after maybeDownloadLspServer');
  // if (analyzerLspPath === undefined) {
  //   return;
  // }

  const context = Context.create(extensionContext, configuration);

  // if (version.stdout && version.stdout.slice(18) !== backend_lastest_version) {
  //   await vscode.window.showWarningMessage(`sui-move-analyzer: The latest version of the language server is ${backend_lastest_version}, but your current version is ${version.stdout.slice(18)}. You can refer to the extension's description page to get the latest version.`);
  // }
  
  // An error here -- for example, if the path to the `sui-move-analyzer` binary that the user
  // specified in their settings is not valid -- prevents the extension from providing any
  // more utility, so return early.
  if (context instanceof Error) {
    void vscode.window.showErrorMessage(
      `Could not activate sui-move-analyzer: ${context.message}.`,
    );
    return;
  }

  // context.registerCommand('textDocumentDocumentSymbol', commands.textDocumentDocumentSymbol);
  context.registerCommand('textDocumentHover', commands.textDocumentHover);
  context.registerCommand('textDocumentCompletion', commands.textDocumentCompletion);
  context.registerCommand('textDocumentDefinition', commands.textDocumentDefinition);

  const d = vscode.languages.registerInlayHintsProvider(
    { scheme: 'file', language: 'move' },
    {
      provideInlayHints(document, range) {
        const client = context.getClient();
        if (client === undefined) {
          return undefined;
        }
        const hints = client.sendRequest<vscode.InlayHint[]>('textDocument/inlayHint',
          { range: range, textDocument: { uri: document.uri.toString() } });
        return hints;
      },
    },
  );

  extensionContext.subscriptions.push(d);
  // Configure other language features.
  context.configureLanguage();

  // All other utilities provided by this extension occur via the language server.
  await context.startClient();

  // Regist all the sui commands.
  Reg.regsui(context);

  // Send inlay hints
  const reload_inlay_hints = function(): any {
    const client = context.getClient();
    if (client !== undefined) {
      void client.sendRequest('move/lsp/client/inlay_hints/config', configuration.inlay_hints_config());
    }
  };
  reload_inlay_hints();
  vscode.workspace.onDidChangeConfiguration(() => {
    log.info('reload_inlay_hints ...  ');
    reload_inlay_hints();
  });
}



// async function ensureServerDownloaded(): Promise<string | undefined> {
//   const installedVersion = getVersionFromMetaFile(
//     os.homedir() + "/.cargo/bin/",
//   );

//   // See if we have the right version
//   // - either its the latest
//   // - or we have the one that's configured
//   let versionToDownload = "";
//   const latestVersion = await getLatestVersion();
//   log.info('latestVersion = ' + latestVersion);
//   if (latestVersion !== installedVersion) {
//     versionToDownload = latestVersion;
//   } else {
//     // Check that the file wasn't unexpectedly removed
//     const lspPath = getLspPath(
//       os.homedir() + "/.cargo/bin/",
//       installedVersion,
//     );
//     log.info('installedVersion = ' + installedVersion);
//     log.info('lspPath = ' + lspPath);
//     if (lspPath === undefined) {
//       versionToDownload = latestVersion;
//     } else {
//       return lspPath;
//     }
//   }
//   log.info('versionToDownload = ' + versionToDownload);
//   log.info('Install the LSP and update the version metadata file');
//   // Install the LSP and update the version metadata file
//   updateStatus("downloading", versionToDownload);
//   const configuration = new Configuration();
//   const newLspPath = await downloadLsp(
//     os.homedir() + "/.cargo/bin/",
//     versionToDownload,
//     configuration.proxyAddr
//   );
//   if (newLspPath === undefined) {
//     updateStatus("error");
//   } else {
//     log.info('newLspPath 0827 = ' + newLspPath);
//     updateStatus("stopped");
//   }
//   return newLspPath;
// }

// async function maybeDownloadLspServer(): Promise<void> {
//   var dest_server_path = path.join(
//     os.homedir() + "/.cargo/bin/",
//     `aptos-move-analyzer`,
//   );
//   if (process.platform === 'win32') {
//     dest_server_path = dest_server_path + '.exe';
//   }
//   const configuration = new Configuration();
//   const userConfiguredAnalyzerLspPath = configuration.serverPath;
//   log.info('userConfiguredAnalyzerLspPath = ' + userConfiguredAnalyzerLspPath);
//   if (
//     userConfiguredAnalyzerLspPath !== dest_server_path
//   ) {
//     log.info('use lsp-server provided by the user');
//     // fs.copyFileSync(userConfiguredAnalyzerLspPath, dest_server_path);
//     analyzerLspPath = userConfiguredAnalyzerLspPath;
//   } else {
//     log.info('before ensureServerDownloaded');
//     analyzerLspPath = await ensureServerDownloaded();
//     if (analyzerLspPath !== undefined) {
//       fs.copyFileSync(analyzerLspPath, dest_server_path);
//     }
//     analyzerLspPath = dest_server_path;
//     log.info('analyzerLspPath = ' + analyzerLspPath);
//   }
// }