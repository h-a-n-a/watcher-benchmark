#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

/**
 * Generate JavaScript file tree with the specified depth
 * Creates index.js and f1.js through f9.js at each level
 * Each fN.js imports its corresponding fN/ subdirectory
 * @param {number} maxDepth - Maximum depth of the tree
 * @param {string} rootDir - Root directory to generate the tree in
 */
function generateFileTree(maxDepth, rootDir = './generated-tree') {
  // Ensure the root directory exists
  if (fs.existsSync(rootDir)) {
    console.log(`Warning: Directory ${rootDir} already exists. Removing it...`);
    fs.rmSync(rootDir, { recursive: true, force: true });
  }

  fs.mkdirSync(rootDir, { recursive: true });

  // Start recursive generation
  generateLevel(rootDir, 1, maxDepth);

  console.log(`✅ File tree generated successfully with depth ${maxDepth} in ${rootDir}`);

  // Count and display statistics
  const stats = countFilesAndDirs(rootDir);
  console.log(`📊 Statistics:`);
  console.log(`   - Total files: ${stats.files}`);
  console.log(`   - Total directories: ${stats.directories}`);
  console.log(`   - Total size: ${formatBytes(stats.size)}`);
}

/**
 * Recursively generate files and directories at each level
 * @param {string} currentPath - Current directory path
 * @param {number} currentDepth - Current depth level
 * @param {number} maxDepth - Maximum depth to generate
 */
function generateLevel(currentPath, currentDepth, maxDepth) {
  // Generate index.js that imports all f1.js through f9.js
  const indexContent = Array.from({ length: 9 }, (_, i) =>
    `import "./f${i + 1}.js"`
  ).join('\n') + '\n';

  fs.writeFileSync(path.join(currentPath, 'index.js'), indexContent);

  // Generate f1.js through f9.js
  for (let i = 1; i <= 9; i++) {
    const fileName = `f${i}.js`;
    const filePath = path.join(currentPath, fileName);

    // If we haven't reached max depth, import the corresponding subdirectory
    if (currentDepth < maxDepth) {
      const content = `import "./f${i}/index.js"\n`;
      fs.writeFileSync(filePath, content);

      // Create the subdirectory and recurse
      const subDirPath = path.join(currentPath, `f${i}`);
      fs.mkdirSync(subDirPath, { recursive: true });

      // Recursively generate the next level
      generateLevel(subDirPath, currentDepth + 1, maxDepth);
    } else {
      // At max depth, just create empty files or minimal content
      fs.writeFileSync(filePath, `// Leaf file at depth ${currentDepth}\n`);
    }
  }
}

/**
 * Count files and directories recursively
 * @param {string} dirPath - Directory path to count
 * @returns {object} Statistics object with file count, directory count, and total size
 */
function countFilesAndDirs(dirPath) {
  let stats = { files: 0, directories: 0, size: 0 };

  const items = fs.readdirSync(dirPath, { withFileTypes: true });

  for (const item of items) {
    const fullPath = path.join(dirPath, item.name);

    if (item.isDirectory()) {
      stats.directories++;
      const subStats = countFilesAndDirs(fullPath);
      stats.files += subStats.files;
      stats.directories += subStats.directories;
      stats.size += subStats.size;
    } else {
      stats.files++;
      const fileStat = fs.statSync(fullPath);
      stats.size += fileStat.size;
    }
  }

  return stats;
}

/**
 * Format bytes to human-readable format
 * @param {number} bytes - Number of bytes
 * @returns {string} Formatted string
 */
function formatBytes(bytes) {
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = bytes;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }

  return `${size.toFixed(2)} ${units[unitIndex]}`;
}

// Parse command line arguments
function main() {
  const args = process.argv.slice(2);

  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.log(`
Usage: node generate-tree.js <depth> [output-dir]

Arguments:
  depth       - Maximum depth of the tree (required, positive integer)
  output-dir  - Output directory path (optional, default: ./generated-tree)

Examples:
  node generate-tree.js 3
  node generate-tree.js 5 ./my-tree
  node generate-tree.js 2 /tmp/test-tree

Note: Be careful with large depth values as the number of files grows exponentially!
  Depth 1: 10 files (index.js + f1.js through f9.js)
  Depth 2: 100 files (10 + 9*10)
  Depth 3: 910 files (10 + 9*10 + 81*10)
  Depth 4: 8,200 files
  Depth 5: 73,810 files
    `);
    process.exit(0);
  }

  const depth = parseInt(args[0], 10);
  const outputDir = args[1] || './generated-tree';

  if (isNaN(depth) || depth < 1) {
    console.error('❌ Error: Depth must be a positive integer');
    process.exit(1);
  }

  if (depth > 5) {
    const estimatedFiles = Math.floor(10 * (Math.pow(9, depth) - 1) / 8);
    console.warn(`⚠️  Warning: Depth ${depth} will generate approximately ${estimatedFiles.toLocaleString()} files!`);
    console.warn('   This may take a long time and use significant disk space.');

    // Ask for confirmation for very large depths
    const readline = require('readline');
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout
    });

    rl.question('   Do you want to continue? (y/N): ', (answer) => {
      rl.close();
      if (answer.toLowerCase() === 'y' || answer.toLowerCase() === 'yes') {
        console.log('\n🚀 Starting generation...\n');
        generateFileTree(depth, outputDir);
      } else {
        console.log('❌ Generation cancelled');
        process.exit(0);
      }
    });
  } else {
    console.log(`🚀 Generating file tree with depth ${depth}...\n`);
    generateFileTree(depth, outputDir);
  }
}

// Run the script
if (require.main === module) {
  main();
}

module.exports = { generateFileTree };