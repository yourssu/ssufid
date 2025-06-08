import * as esbuild from "npm:esbuild";
import { denoPlugins } from "jsr:@luca/esbuild-deno-loader";

const _result = await esbuild.build({
  plugins: [
    ...denoPlugins({
      loader: "native",
    }),
  ],
  platform: "node",
  entryPoints: ["./src/main.ts"],
  outfile: "./dist/lexical-parser.esm.js",
  bundle: true,
  format: "esm",
});

esbuild.stop();
