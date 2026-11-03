import { defineCollection, z } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

export const collections = {
  docs: defineCollection({
    loader: docsLoader(),
    schema: docsSchema({
      extend: z.object({
        track: z.enum(["copilot", "agent"]).optional(),
        stage: z.enum(["diagnose", "isolate", "prove", "improve", "result"]).optional(),
        verified: z.coerce.date().optional(),
      }),
    }),
  }),
};
