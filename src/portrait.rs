use image::RgbaImage;

/// Remove a near-solid matte connected to the edge of an opaque portrait.
/// Leave the canvas size intact so positions in the stage do not change.
pub(crate) fn remove_matte(image: &mut RgbaImage) -> bool {
    let (width, height) = image.dimensions();
    if width < 3 || height < 3 || image.pixels().any(|pixel| pixel[3] != 255) {
        return false;
    }
    let corners = [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ];
    let close =
        |pixel: &[u8; 4], matte: &[u8; 4]| (0..3).all(|i| pixel[i].abs_diff(matte[i]) <= 24);
    let edges = || {
        (0..width)
            .flat_map(|x| [(x, 0), (x, height - 1)])
            .chain((1..height - 1).flat_map(|y| [(0, y), (width - 1, y)]))
    };
    let Some(matte) = corners.iter().map(|&(x, y)| image[(x, y)].0).find(|matte| {
        corners
            .iter()
            .filter(|&&(x, y)| close(&image[(x, y)].0, matte))
            .count()
            >= 2
            && edges()
                .filter(|&(x, y)| close(&image[(x, y)].0, matte))
                .count()
                * 3
                >= (2 * (width + height) - 4) as usize * 2
    }) else {
        return false;
    };
    let mut removed = 0;
    let mut pending = Vec::new();
    let visit = |image: &mut RgbaImage, pending: &mut Vec<u32>, x, y| {
        let pixel = &mut image[(x, y)].0;
        if pixel[3] == 0 || !close(pixel, &matte) {
            return false;
        }
        *pixel = [0; 4];
        pending.push(y * width + x);
        true
    };
    for (x, y) in edges() {
        removed += u32::from(visit(image, &mut pending, x, y));
    }
    while let Some(index) = pending.pop() {
        let (x, y) = (index % width, index / width);
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx < width && ny < height && visit(image, &mut pending, nx, ny) {
                removed += 1;
            }
        }
    }
    // An entirely flat image or a minute border is not a reliable subject.
    removed >= width * height / 30 && removed < width * height
}
