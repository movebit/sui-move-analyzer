// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as os from 'os';
import * as vscode from 'vscode';
import * as Path from 'path';

class InlayHintsConfig {
    field_type: boolean;

    parameter: boolean;

    declare_var: boolean;

    constructor(fieldType: boolean,
        parameter: boolean,
        declareVar: boolean) {
        this.field_type = fieldType;
        this.parameter = parameter;
        this.declare_var = declareVar;
    }
}

/**
 * User-defined configuration values, such as those specified in VS Code settings.
 *
 * This provides a more strongly typed interface to the configuration values specified in this
 * extension's `package.json`, under the key `"contributes.configuration.properties"`.
 */


class Configuration {
    private readonly configuration: vscode.WorkspaceConfiguration;

    constructor() {
        this.configuration = vscode.workspace.getConfiguration('sui-move-analyzer');
    }

    /** A string representation of the configured values, for logging purposes. */
    toString(): string {
        return JSON.stringify(this.configuration);
    }

    /** The path to the sui-move-analyzer executable. */
    get serverPath(): string {
        const defaultName = 'sui-move-analyzer';
        let serverPath = this.configuration.get<string>('server.path', defaultName);
        if (serverPath.length === 0) {
            // The default value of the `server.path` setting is 'sui-move-analyzer'.
            // A user may have over-written this default with an empty string value, ''.
            // An empty string cannot be an executable name, so instead use the default.
            return defaultName;
        }

        if (serverPath === defaultName) {
            // If the program set by the user is through PATH,
            // it will return directly if specified
            return defaultName;
        }

        if (serverPath.startsWith('~/')) {
            serverPath = os.homedir() + serverPath.slice('~'.length);
        }

        if (process.platform === 'win32' && !serverPath.endsWith('.exe')) {
            serverPath = serverPath + '.exe';
        }
        return Path.resolve(serverPath);
    }

    inlay_hints_config(): InlayHintsConfig {
        const ft = this.configuration.get<boolean>('inlay.hints.field.type');

        const p = this.configuration.get<boolean>('inlay.hints.parameter');

        const dv = this.configuration.get<boolean>('inlay.hints.declare.var');

        return new InlayHintsConfig(ft === true ? ft : false, p === true ? p : false, dv === true ? dv : false);
    }
}

export { InlayHintsConfig, Configuration };
