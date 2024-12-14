import * as vscode from "vscode";
import { format } from "./commands/format";
import {checkErrors} from "./commands/errorCheck";

const pyre = vscode.window.createOutputChannel("Pyre");

export function activate(context: vscode.ExtensionContext) {


  // Register the formatting provider
  const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider(
    "pyre",
    {
      async provideDocumentFormattingEdits(
        document: vscode.TextDocument,
      ): Promise<vscode.TextEdit[]> {
        try {
          pyre.appendLine("Formatting")
          const text = document.getText();
          const formattedText = await format(text, document.fileName);
         
          const fullRange = new vscode.Range(
            document.positionAt(0),
            document.positionAt(document.getText().length),
          );
         


          return [vscode.TextEdit.replace(fullRange, formattedText)];
        } catch (error) {
          pyre.appendLine(`Error formatting Pyre file: ${error}`)
          vscode.window.showErrorMessage(
            `Error formatting Pyre file: ${error}`,
          );
          return [];
        }
      },
    },
  );
  context.subscriptions.push(formattingProvider);



  // Dignostics
  const diagnostics = vscode.languages.createDiagnosticCollection('pyre');
  context.subscriptions.push(diagnostics);

  // Listen for document save events and trigger diagnostics
  vscode.workspace.onDidSaveTextDocument((document) => {
    if (document.languageId === 'pyre') {
      checkErrors(document, diagnostics);
    }
  });
}


export function deactivate() { }
