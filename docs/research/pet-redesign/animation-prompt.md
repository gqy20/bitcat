# 待机关键帧生成提示词

使用内置 image_gen，参考图为 `tuxedo-idle-v1.png`。

Create a production animation SPRITE SHEET from the supplied black-and-white BitCat reference. Use case: identity-preserve animation asset. Exactly FOUR frames in a perfectly regular 2 columns by 2 rows grid, each cell equal size, no gaps between cells, no labels, no borders. Overall square PNG with genuine transparent alpha background. Each cell contains exactly one complete cat at IDENTICAL position and scale, paws baseline and head alignment identical across all four cells. Cat occupies 80 percent of each cell height. Preserve reference identity: tall ears, slim seated tuxedo body, asymmetrical ivory nose blaze, white chest and socks, narrow amber eyes, tall hook tail on viewer right with white tip. Preserve this pose and proportions. Consistent crisp square-pixel art with flat discrete color clusters, no textured shading, no antialiasing, no glossy effects. Simplify tiny whisker detail.
FRAME ORDER row-major:
top-left: neutral idle, amber eyes open as reference, tail neutral;
top-right: SAME IMAGE with ONLY BOTH eyes fully closed in relaxed thin dark eyelid lines, amber completely hidden; tail neutral;
bottom-left: SAME as top-left, eyes open, ONLY upper hook and white tail tip gently bent left toward cat by one logical pixel; tail base fixed;
bottom-right: SAME as top-left, eyes open, ONLY upper hook and white tail tip gently bent right away from cat by one logical pixel; tail base fixed.
All pixels outside eyes and upper tail must be identical between cells. Absolutely no head motion, body motion, changing chest patch, changing ears, extra paws, text, backdrop, drop shadow, perspective changes. These are frames for one subtle looping idle animation, not four different cats.
