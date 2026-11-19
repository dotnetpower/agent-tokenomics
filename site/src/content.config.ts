import { defineCollection, z } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

const documentDate = z.union([
  z.string().date(),
  z.date().transform((date) => date.toISOString().slice(0, 10)),
]);

export const collections = {
  docs: defineCollection({
    loader: docsLoader(),
    schema: (context) => docsSchema({
      extend: z.object({
        track: z.enum(["copilot", "agent"]).optional(),
        stage: z.enum(["diagnose", "isolate", "prove", "improve", "result"]).optional(),
        created: documentDate,
        updated: documentDate,
        dateSource: z.enum(["git", "recorded"]),
        verified: z.coerce.date().optional(),
      }),
    })(context).superRefine((data, ctx) => {
      if (data.created > data.updated)
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["updated"], message: "updated must not precede created" });
    }),
  }),
};
