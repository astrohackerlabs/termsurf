import fs from "fs";
import path from "path";
import sharp from "sharp";
import pngToIco from "png-to-ico";

const __dirname = path.dirname(new URL(import.meta.url).pathname);

const sourceDir = path.resolve(__dirname, "../raw-icons");
const outputDir = path.resolve(__dirname, "../public/images");
const publicDir = path.resolve(__dirname, "../public");
const outputTsFile = path.resolve(__dirname, "../src/util/icons.ts");

const formatConfigs = [
  { format: "png", sizes: [192], extension: "png" },
  { format: "ico", sizes: [32], extension: "ico" },
] as const;

// Source icon used for favicon.
const faviconSource = "termsurf-11-transparent";

const outputPaths: string[] = [];

fs.mkdirSync(outputDir, { recursive: true });
fs.mkdirSync(path.dirname(outputTsFile), { recursive: true });

async function processFile(file: string) {
  const baseName = path.basename(file, ".png");
  const sourcePath = path.join(sourceDir, file);

  for (const { format, sizes, extension } of formatConfigs) {
    for (const size of sizes) {
      const outputFileName = `${baseName}-${size}.${extension}`;
      const outputPath = path.join(outputDir, outputFileName);
      const webPath = `/images/${outputFileName}`;

      outputPaths.push(webPath);

      if (format === "ico") {
        const buffer = await sharp(sourcePath)
          .resize(size, size)
          .toFormat("png")
          .toBuffer();
        const icoBuffer = await pngToIco([buffer]);
        fs.writeFileSync(outputPath, new Uint8Array(icoBuffer));
      } else {
        await sharp(sourcePath)
          .resize({ height: size })
          .toFormat(format as keyof sharp.FormatEnum)
          .toFile(outputPath);
      }

      console.log(`Processed ${outputFileName}`);
    }
  }

  // Copy favicon.
  if (baseName === faviconSource) {
    const icoFile = `${baseName}-32.ico`;
    const icoPath = path.join(outputDir, icoFile);
    const faviconPath = path.join(publicDir, "favicon.ico");
    fs.copyFileSync(icoPath, faviconPath);
    console.log(`Copied favicon.ico`);
  }
}

async function main() {
  const files = fs
    .readdirSync(sourceDir)
    .filter((file) => path.extname(file).toLowerCase() === ".png");

  for (const file of files) {
    await processFile(file);
  }

  const typeContent = `export type Icon =\n  ${outputPaths
    .sort()
    .map((p) => `| "${p}"`)
    .join("\n  ")};\nexport const $icon = (icon: Icon) => icon;\n`;

  fs.writeFileSync(outputTsFile, typeContent);
  console.log(`Generated types file at ${outputTsFile}`);
}

main().catch(console.error);
