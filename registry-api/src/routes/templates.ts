import express, { Request, Response } from "express";
import { v4 as uuid } from "uuid";
import { TemplateStore, ITemplate } from "../models/Template";
import { ReviewStore } from "../models/Review";
import { verifyToken, optionalAuth } from "../middleware/auth";
import logger from "../utils/logger";
import fs from "fs";
import path from "path";

const router = express.Router();
const templateStore = new TemplateStore();
const reviewStore = new ReviewStore();

const STORAGE_DIR = process.env.STORAGE_DIR || "./storage/templates";

// Ensure storage directory exists
if (!fs.existsSync(STORAGE_DIR)) {
  fs.mkdirSync(STORAGE_DIR, { recursive: true });
}

// Search templates
router.post("/search", optionalAuth, async (req: Request, res: Response) => {
  try {
    const {
      query = "",
      tags,
      verified,
      min_quality,
      limit = 20,
      offset = 0,
    } = req.body;

    const results = await templateStore.search(query, tags, verified);
    const paginated = results.slice(offset, offset + limit);

    res.json({
      success: true,
      results: paginated.map((tpl) => ({
        id: tpl.id,
        name: tpl.name,
        version: tpl.version,
        description: tpl.description,
        author: tpl.author,
        tags: tpl.tags,
        license: tpl.license,
        repository: tpl.repository,
        homepage: tpl.homepage,
        documentation: tpl.documentation,
        downloads: tpl.downloads,
        verified: tpl.verified,
        created_at: tpl.createdAt,
        updated_at: tpl.updatedAt,
        ratings: {
          average_rating: tpl.ratings.average,
          review_count: tpl.ratings.count,
          five_star: tpl.ratings.distribution[5] || 0,
          four_star: tpl.ratings.distribution[4] || 0,
          three_star: tpl.ratings.distribution[3] || 0,
          two_star: tpl.ratings.distribution[2] || 0,
          one_star: tpl.ratings.distribution[1] || 0,
        },
        download_url: tpl.downloadUrl,
      })),
      total: results.length,
      limit,
      offset,
    });
  } catch (err) {
    logger.error("Search error", err);
    res.status(500).json({ error: "Search failed" });
  }
});

// Get template by name and version
router.get(
  "/:name/:version",
  optionalAuth,
  async (req: Request, res: Response) => {
    try {
      const { name, version } = req.params;
      const versionQuery = version === "latest" ? undefined : version;

      const results = await templateStore.findByName(name);
      if (results.length === 0) {
        return res.status(404).json({ error: "Template not found" });
      }

      let tpl = results[0];
      if (versionQuery) {
        tpl = results.find((t) => t.version === versionQuery) || results[0];
      }

      res.json({
        id: tpl.id,
        name: tpl.name,
        version: tpl.version,
        description: tpl.description,
        author: tpl.author,
        tags: tpl.tags,
        license: tpl.license,
        repository: tpl.repository,
        homepage: tpl.homepage,
        documentation: tpl.documentation,
        downloads: tpl.downloads,
        verified: tpl.verified,
        created_at: tpl.createdAt,
        updated_at: tpl.updatedAt,
        ratings: {
          average_rating: tpl.ratings.average,
          review_count: tpl.ratings.count,
          five_star: tpl.ratings.distribution[5] || 0,
          four_star: tpl.ratings.distribution[4] || 0,
          three_star: tpl.ratings.distribution[3] || 0,
          two_star: tpl.ratings.distribution[2] || 0,
          one_star: tpl.ratings.distribution[1] || 0,
        },
        download_url: tpl.downloadUrl,
      });
    } catch (err) {
      logger.error("Get template error", err);
      res.status(500).json({ error: "Failed to fetch template" });
    }
  },
);

// Publish template
router.post("/publish", verifyToken, async (req: Request, res: Response) => {
  try {
    const {
      name,
      version,
      description,
      author,
      tags,
      license,
      repository,
      homepage,
      documentation,
      content,
    } = req.body;

    if (!name || !version || !description || !author || !content) {
      return res.status(400).json({ error: "Missing required fields" });
    }

    // Check if template already exists
    const existing = await templateStore.findByNameAndVersion(name, version);
    if (existing && existing.publisherId === req.userId) {
      return res
        .status(409)
        .json({ error: "Template version already published" });
    }

    // Save template content
    const templateId = uuid();
    const fileName = `${name}-${version}-${templateId}.zip`;
    const filePath = path.join(STORAGE_DIR, fileName);

    const buffer = Buffer.from(content, "base64");
    fs.writeFileSync(filePath, buffer);

    const template: ITemplate = {
      id: templateId,
      name,
      version,
      description,
      author,
      tags: tags || [],
      license,
      repository,
      homepage,
      documentation,
      downloads: 0,
      verified: false,
      publisherId: req.userId!,
      createdAt: new Date(),
      updatedAt: new Date(),
      ratings: { average: 0, count: 0, distribution: {} },
      downloadUrl: `/api/templates/${name}/${version}/download`,
    };

    await templateStore.create(template);
    logger.info(`Template published: ${name}@${version}`);

    res.status(201).json({
      success: true,
      message: "Template published successfully",
      template_id: templateId,
      url: `/registry/template/${name}/${version}`,
    });
  } catch (err) {
    logger.error("Publish error", err);
    res.status(500).json({ error: "Publish failed" });
  }
});

// Download template
router.get(
  "/:name/:version/download",
  optionalAuth,
  async (req: Request, res: Response) => {
    try {
      const { name, version } = req.params;

      const results = await templateStore.findByName(name);
      const tpl = results.find((t) => t.version === version) || results[0];

      if (!tpl) {
        return res.status(404).json({ error: "Template not found" });
      }

      await templateStore.incrementDownloads(tpl.id);

      const fileName = path.basename(tpl.downloadUrl);
      const filePath = path.join(
        STORAGE_DIR,
        `${tpl.name}-${tpl.version}-${tpl.id}.zip`,
      );

      if (!fs.existsSync(filePath)) {
        return res.status(404).json({ error: "Template file not found" });
      }

      res.download(filePath, `${tpl.name}-${tpl.version}.zip`);
    } catch (err) {
      logger.error("Download error", err);
      res.status(500).json({ error: "Download failed" });
    }
  },
);

export default router;
