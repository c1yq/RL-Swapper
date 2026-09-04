# Rocket League Asset Swapper

A powerful, easy-to-use tool to inject custom images and textures directly into Rocket League's game files. Swap out in-game Profile Pictures, Player Banners, Decals, and more with your own custom images (PNG, JPG, WEBP, etc.)!

## Features & Tools

### 1. Profile Picture Swapper
Inject your own custom avatar directly into the game's UI.
* **Recommended In-Game Border to Equip:** XP Level 25 *(Note: The true "Default" border does not properly display the modded texture).*
* **Optimal Image Size:** 84x84 pixels.
* **How it works:** The tool automatically extracts the game's native silver border, shrinks your custom image to perfectly fit the frame, and composites the native border back over your image so it looks completely natural in the Rocket League menus.

### 2. Player Banner Swapper
Replace an existing Rocket League player banner with any image of your choice.
* **Recommended In-Game Banner to Equip:** Topographic.
* **Optimal Image Size:** 420x90 pixels.
* **How it works:** Rocket League banners natively render at 420x100. Our built-in cropping tool locks you into a flawless 420x90 aspect ratio, and the swapper automatically pads the remaining 10 pixels with transparency. This perfectly counteracts the game's UI stretching, making your banner look exactly as slim and sleek as the native Taxi banner.

### 3. Decal / General Asset Swapper
Swap car decals, ball textures, and other standard textures.
* **How it works:** Select the original texture you want to replace, choose your custom image, and the tool will handle the decompression, format conversion, and byte-injection necessary to bypass Rocket League's file verification.

### 4. Backup & Restore
* **Restore All Backups:** Bypassing the game's file verification requires modifying the raw .upk files in your Rocket League installation. The swapper automatically creates .bak backups of every file it touches. If your game crashes or you want to revert to the original textures, simply click "Restore All Backups".

## Installation & Usage

1. Download the latest release .exe.
2. Run the program as Administrator (required for modifying files in Program Files).
3. Follow the on-screen prompts to select your custom images and inject them.
4. Launch Rocket League!

*Disclaimer: Modifying game files is done at your own risk. This tool modifies UI textures locally. Other players will not see your custom textures.*
