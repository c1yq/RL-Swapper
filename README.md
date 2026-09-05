# Rocket League Asset Swapper
![Total Downloads](https://img.shields.io/github/downloads/YOUR_USERNAME/YOUR_REPOSITORY_NAME/total.svg?style=for-the-badge&color=blue)

*(Note to Dev: To make the download counter work, replace YOUR_USERNAME and YOUR_REPOSITORY_NAME in the link above with your actual GitHub username and repository name once you publish it!)*

A powerful, easy-to-use tool to inject custom images and textures directly into Rocket League's game files. Swap out in-game Profile Pictures, Player Banners, Decals, and more with your own custom images (PNG, JPG, WEBP, BMP)!

## Features & Tools

### 1. Profile Picture Swapper
Inject your own custom avatar directly into the game's UI.
* **How it works:** The tool automatically extracts the game's native silver border, shrinks your custom image to perfectly fit the frame, and composites the native border back over your image so it looks completely natural in the Rocket League menus.
* **Recommended In-Game Border to Equip:** XP Level 25 *(Note: The true Level 1 "Default" border does not have a texture frame, so you cannot inject into it. Pick a real border!)*
* **Optimal Image Size:** 84x84 pixels.

### 2. Player Banner Swapper
Replace an existing Rocket League player banner with any image of your choice.
* **How it works:** Rocket League banners natively render at 420x100. Our built-in cropping tool locks you into a flawless 14:3 aspect ratio, and the swapper automatically pads the remaining pixels with transparency. This makes your banner look exactly as slim and sleek as the native banners without getting vertically stretched!
* **Recommended In-Game Banner to Equip:** Topographic or Standard.
* **Optimal Image Size:** 420x90 pixels.

### 3. Bulk Randomizer (Troll Your Opponents!)
Want to see funny pictures every time the opponent scores? 
* **How it works:** Instead of replacing just one border or banner, the Bulk Randomizer lets you select an entire folder of pictures from your PC. It will scan the game files and automatically inject a randomly chosen picture into **EVERY SINGLE BORDER** (348+ files) and **EVERY SINGLE BANNER** (1400+ files) in the game! 
* No matter what rank the opponent is, or what item they have equipped, they will show up with one of your custom pictures! 

### 4. Smart Auto-Restore Engine
* **How it works:** You never have to worry about breaking or corrupting your game files. If you try to swap an item that you've already modded, the tool will silently restore the pristine original backup file behind the scenes *before* applying your new swap. 

### 5. Dedicated Restore Menu
* **How it works:** If you want to undo a swap, click "Restore Items" in the top left. Instead of deleting all your mods at once, a brand new menu will pop up letting you check off *exactly* which files you want to revert to normal!

## Installation & Usage

1. Download the latest release .exe (or run SwapperGUI.py if you have Python installed).
2. Run the program (Administrator rights may be required depending on where your game is installed).
3. The tool will automatically locate your Rocket League installation.
4. Select your custom images, hit Inject, and Launch Rocket League!

*Disclaimer: Modifying game files is done at your own risk. This tool modifies UI textures locally. Only you will see the custom textures.*
