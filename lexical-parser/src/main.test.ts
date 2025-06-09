import { assertEquals } from "jsr:@std/assert";
import { toHtml } from "./main.ts";

Deno.test(async function simpleHtmlTest() {
  const exampleState = `{"root":{"children":[{"children":[],"direction":null,"format":"","indent":0,"type":"paragraph","version":1}],"direction":null,"format":"","indent":0,"type":"root","version":1}}`;
  const output = await toHtml(exampleState);
  console.log(output);
  assertEquals(output, "<p><br></p>");
});
