// @ts-nocheck: Lexical type check is broken
import { parseHTML } from "npm:linkedom";

const dom = parseHTML(`<html><body></body></html>`);

globalThis.window = dom.window;
// @ts-ignore: lexical uses `document` in a way that is not compatible with TypeScript
globalThis.document = dom.window.document;

import { $generateHtmlFromNodes } from "npm:@lexical/html";
import { AutoLinkNode, LinkNode } from "npm:@lexical/link";
import { ListItemNode, ListNode } from "npm:@lexical/list";
import { HeadingNode, QuoteNode } from "npm:@lexical/rich-text";
import { TableCellNode, TableNode, TableRowNode } from "npm:@lexical/table";
import { ImageNode } from "./nodes/ImageNode.tsx";
import { InlineImageNode } from "./nodes/InlineImageNode.tsx";
import { HorizontalRuleNode } from "./nodes/HorizontalRuleNode.tsx";
import { createHeadlessEditor } from "npm:@lexical/headless";

export const toHtml = async (stateStr: string) => {
  const editor = createHeadlessEditor({
    nodes: [
      HeadingNode,
      ListNode,
      ListItemNode,
      QuoteNode,
      TableNode,
      TableCellNode,
      TableRowNode,
      AutoLinkNode,
      LinkNode,
      ImageNode,
      InlineImageNode,
      HorizontalRuleNode,
    ],
  });
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

Deno.serve(async (req) => {
  if (req.method !== "POST") {
    return new Response("Method Not Allowed", { status: 405 });
  }
  const rawBody = await req.text();
  if (rawBody.startsWith("<"))
    return new Response("<div>redacted</div>", {
      headers: { "Content-Type": "text/html" },
    });
  const stateStr = JSON.parse(rawBody);
  const stateObj = JSON.parse(stateStr);
  const state = stateObj.editorState;
  if (!state) {
    return new Response("Bad Request: No body provided", { status: 400 });
  }

  return new Response(await toHtml(state), {
    headers: { "Content-Type": "text/html" },
  });
});
