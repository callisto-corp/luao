import * as vscode from "vscode";
import * as path from "path";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function findLuaoBinary(): string {
  const config = vscode.workspace.getConfiguration("luao");
  const configPath = config.get<string>("binaryPath");
  if (configPath) {
    return configPath;
  }
  return "luao";
}

export async function activate(
  context: vscode.ExtensionContext
): Promise<void> {
  const command = findLuaoBinary();

  const serverOptions: ServerOptions = {
    run: { command, args: ["lsp"] },
    debug: { command, args: ["lsp"] },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "luao" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.luao"),
    },
  };

  client = new LanguageClient(
    "luao",
    "Luao Language Server",
    serverOptions,
    clientOptions
  );

  await client.start();
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
