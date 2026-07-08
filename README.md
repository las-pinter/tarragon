# Tarragon

**Browse thousands of images instantly, without moving a single file.**

Tarragon is a fast, free image browser built for artists who are drowning in reference images, sketches, and PSD files. Point it at a folder, and start browsing immediately. No importing. No copying. No waiting.

Your files stay exactly where they are on your hard drive. Tarragon just reads them, fast.

## Why Tarragon?

You know the feeling. You have 10,000 reference images scattered across a dozen folders. You need to find *that one* blue-toned landscape you saved last month. You're scrolling through file explorers, opening Photoshop just to preview a PSD, and wasting time instead of making art.

Tarragon fixes that.

Open a folder of 5,000 PSDs and browse them instantly as thumbnails. Find images by the colors they contain. Tag your favorites. Double-click to open in Photoshop, GIMP, or Krita. It's the image browser that actually understands how artists work.

## Features

### 🖼️ Instant Browsing

Point Tarragon at any folder and see your images as thumbnails, immediately. JPEG, PNG, WebP, TIFF, and even PSD/PSB files. No importing, no copying, no waiting for a library to build. Your files stay right where they are.

### 🎨 Find Images by Color

Remember saving an image because of its color palette? Tarragon does too. It automatically detects the dominant colors in every image: red, blue, green, purple, and more. Click a color swatch to filter your collection and find exactly what you're looking for.

### 📂 Photoshop Files Without Photoshop

Preview layered PSD and PSB files without launching Photoshop. See thumbnails, check dimensions, and browse your PSD collections as easily as regular images. When you're ready to edit, double-click to open in your editor of choice.

### 🏷️ Organize Your Way

Create your own tags and apply them to any image. Build a tagging system that makes sense for *your* workflow: by project, by subject, by mood, by client. Filter by multiple tags at once to narrow down your search.

### ⭐ Favorites

Got folders you open every day? Add them to your Favorites sidebar for one-click access. Your reference library, your WIP folder, your client deliverables, all just a click away.

### ✏️ Open in Your Editor

Double-click any image to open it in Photoshop, GIMP, Krita, or whatever you use. Configure custom editor commands for different file types, or just let Tarragon use your system default.

### 🌙 Dark Theme

A dark, easy-on-the-eyes interface designed for long work sessions. Because staring at a bright white window for eight hours isn't doing anyone any favors.

### ⚡ Actually Fast

Tarragon is a compiled native application, not a web app running in a browser. Thumbnails are cached so they load instantly the second time. Large folders with thousands of files stay responsive. It's built to handle real-world art collections.

## Quick Start

1. **Install Tarragon** (see below)
2. **Launch it**: you'll see the Library, Gallery, and Preview panels
3. **Open a folder**: File → Open Folder, and pick any directory with images
4. **Browse**: scroll through thumbnails, click to preview, double-click to open in your editor
5. **Organize**: add tags, mark favorites, filter by color

That's it. No library to set up. No database to configure. Just open a folder and go.

## Installation

### From Source (Developer Setup)

Tarragon requires Python 3.12 or later.

```bash
# Clone the repository
git clone https://github.com/las-pinter/tarragon.git
cd tarragon

# Install in editable mode
pip install -e .

# Launch Tarragon
python -m tarragon
```

### Pre-built Binaries

Standalone binaries for Windows, macOS, and Linux are coming soon. These won't require Python, just download and run.

### Build Your Own Binary

See [Release Build Instructions](docs/release.md) for how to build a standalone binary.

## For Developers

See [CONTRIBUTING.md](CONTRIBUTING.md) for developer setup, project architecture, and contribution guidelines.

## Screenshots

*Coming soon, we're polishing the interface for its close-up.*

## License

Tarragon is open source software licensed under the [MIT License](LICENSE). Free to use, modify, and share.
