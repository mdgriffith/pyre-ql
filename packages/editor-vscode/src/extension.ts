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

  const checkTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const scheduleCheck = (document: vscode.TextDocument) => {
    if (document.languageId !== 'pyre') {
      return;
    }

    const key = document.uri.toString();
    const existing = checkTimers.get(key);
    if (existing) {
      clearTimeout(existing);
    }

    const timer = setTimeout(() => {
      checkTimers.delete(key);
      checkErrors(document, diagnostics);
    }, 250);

    checkTimers.set(key, timer);
  };

  // Listen for document save events and trigger diagnostics
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((document) => {
      scheduleCheck(document);
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      scheduleCheck(document);
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument((event) => {
      scheduleCheck(event.document);
    }),
  );

  context.subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((document) => {
      diagnostics.delete(document.uri);
      const key = document.uri.toString();
      const existing = checkTimers.get(key);
      if (existing) {
        clearTimeout(existing);
        checkTimers.delete(key);
      }
    }),
  );

  const active = vscode.window.activeTextEditor;
  if (active) {
    scheduleCheck(active.document);
  }
}


export function deactivate() { }
