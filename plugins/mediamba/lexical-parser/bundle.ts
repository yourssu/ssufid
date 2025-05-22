import * as esbuild from "npm:esbuild";
import { denoPlugins } from "jsr:@luca/esbuild-deno-loader";

const _result = await esbuild.build({
  plugins: [
    ...denoPlugins({
      loader: "native",
    }),
  ],
  entryPoints: ["./src/main.ts"],
  outfile: "./dist/lexcial-parser.esm.js",
  bundle: true,
  format: "esm",
});

esbuild.stop();
