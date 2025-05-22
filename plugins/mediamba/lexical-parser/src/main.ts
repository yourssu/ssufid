import { JSDOM } from "https://jspm.dev/jsdom";
import { createHeadlessEditor } from "npm:@lexical/headless";
import { $generateHtmlFromNodes } from "npm:@lexical/html";

const dom = new JSDOM();

const editor = createHeadlessEditor({ nodes: [] });

globalThis.window = dom.window;
// @ts-ignore: lexical uses `document` in a way that is not compatible with TypeScript
globalThis.document = dom.window.document;

export const toHtml = async (stateStr: string) => {
  const state = editor.parseEditorState(stateStr);
  await new Promise<void>((resolve) =>
    editor.update(() => {
      editor.setEditorState(state);
      resolve();
    })
  );
  return new Promise<string>((resolve) => {
    editor.read(() => {
      resolve($generateHtmlFromNodes(editor));
    });
  });
};
