import * as vscode from "vscode";
import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);

export function activate(context: vscode.ExtensionContext) {
  // Register the formatting provider
  const provider = vscode.languages.registerDocumentFormattingEditProvider(
    "pyre",
    {
      async provideDocumentFormattingEdits(
        document: vscode.TextDocument
      ): Promise<vscode.TextEdit[]> {
        const filePath = document.uri.fsPath;

        try {
          const { stdout } = await execAsync(
            `pyre format ${filePath} --to-stdout`
          );
          const formattedText = stdout;

          const fullRange = new vscode.Range(
            document.positionAt(0),
            document.positionAt(document.getText().length)
          );

          return [vscode.TextEdit.replace(fullRange, formattedText)];
        } catch (error) {
          vscode.window.showErrorMessage(
            `Error formatting Pyre file: ${error}`
          );
          return [];
        }
      },
    }
  );

  // Register the command
  let disposable = vscode.commands.registerCommand("pyre.format", () => {
    const editor = vscode.window.activeTextEditor;
    if (editor) {
      vscode.commands.executeCommand("editor.action.formatDocument");
    }
  });

  context.subscriptions.push(provider, disposable);
}

export function deactivate() {}
