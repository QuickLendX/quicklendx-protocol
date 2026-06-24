import path from "path";
import { promises as fs } from "fs";
import * as zlib from "zlib";
import { createHash } from "crypto";
import { config } from "../src/config";
import { FileRawEventStore } from "../src/services/rawEventStore";
import { DefaultEventValidator } from "../src/services/eventValidator";
import { RawEvent } from "../src/types/replay";

export async function restoreArchivedEvents(options: {
  start: string;
  end: string;
  archiveDir?: string;
  rawEventStore?: any;
}): Promise<number> {
  const startDate = new Date(options.start);
  const endDate = new Date(options.end);

  if (isNaN(startDate.getTime()) || isNaN(endDate.getTime())) {
    throw new Error("Invalid start or end date format.");
  }

  const archiveDir = options.archiveDir ?? config.ARCHIVE_DIR;
  const store = options.rawEventStore ?? new FileRawEventStore(new DefaultEventValidator());

  let files: string[] = [];
  try {
    files = await fs.readdir(archiveDir);
  } catch (error) {
    if ((error as any).code === "ENOENT") {
      return 0;
    }
    throw error;
  }

  // Get existing event IDs to ensure idempotency
  const existingEvents = await store.getAllEvents();
  const existingIds = new Set(existingEvents.map((e: any) => e.id));

  let totalRestored = 0;

  for (const fileName of files) {
    const match = fileName.match(/^raw-events-(\d{4}-\d{2})\.jsonl\.gz$/);
    if (!match) continue;

    const filePath = path.join(archiveDir, fileName);
    const checksumPath = `${filePath}.sha256`;
    let expectedChecksum: string;
    try {
      expectedChecksum = (await fs.readFile(checksumPath, "utf8")).trim();
    } catch (err) {
      throw new Error(`Checksum file missing for ${filePath}`);
    }

    const fileBuffer = await fs.readFile(filePath);
    const actualChecksum = createHash("sha256").update(fileBuffer).digest("hex");
    if (actualChecksum !== expectedChecksum) {
      throw new Error(`Checksum verification failed for ${filePath}`);
    }

    let decompressed: Buffer;
    try {
      decompressed = await new Promise<Buffer>((resolve, reject) => {
        zlib.gunzip(fileBuffer, (err, result) => {
          if (err) reject(err);
          else resolve(result);
        });
      });
    } catch (err: any) {
      throw new Error(`Failed to decompress ${filePath}: ${err.message}`);
    }

    const text = decompressed.toString("utf8");
    const lines = text.split("\n").filter((l) => l.trim().length > 0);
    const parsedEvents: RawEvent[] = [];
    for (const line of lines) {
      try {
        parsedEvents.push(JSON.parse(line) as RawEvent);
      } catch (err: any) {
        throw new Error(`Failed to parse JSON line from ${filePath}: ${err.message}`);
      }
    }

    const eventsToRestore = parsedEvents.filter((e) => {
      const eventDate = new Date(e.indexedAt);
      return eventDate >= startDate && eventDate <= endDate;
    });

    const newEvents = eventsToRestore.filter((e) => !existingIds.has(e.id));
    if (newEvents.length > 0) {
      await store.storeEvents(newEvents);
      totalRestored += newEvents.length;
    }
  }

  return totalRestored;
}

if (require.main === module) {
  const yargs = require("yargs");
  const { hideBin } = require("yargs/helpers");

  const argv = yargs(hideBin(process.argv))
    .options({
      start: { type: "string", demandOption: true, describe: "Start date (ISO or YYYY-MM-DD)" },
      end: { type: "string", demandOption: true, describe: "End date (ISO or YYYY-MM-DD)" },
    })
    .parseSync();

  restoreArchivedEvents({ start: argv.start, end: argv.end })
    .then((count) => {
      console.log(`Successfully restored ${count} raw events.`);
      process.exit(0);
    })
    .catch((error) => {
      console.error("Restoration failed:", error.message);
      process.exit(1);
    });
}
