use std::io::{BufRead, Cursor, Seek};
use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageOutputFormat, Pixel};
use leptess::capi::Box;
use leptess::LepTess;

pub enum Format {}

pub fn extract_table_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<Vec<String>>> {
  extract_table_image(image::open(path)?)
}

pub fn extract_table_png_buf<R: BufRead + Seek>(buf: R) -> anyhow::Result<Vec<Vec<String>>> {
  extract_table_image(image::load(buf, ImageFormat::Png)?)
}

pub fn extract_table_image(image: DynamicImage) -> anyhow::Result<Vec<Vec<String>>> {
  let image = image.adjust_contrast(20.0).to_luma8();

  let mut ylines = Vec::new();

  for x in 0..image.width() {
    let mut start = None;

    for y in 0..image.height() {
      if *image.get_pixel(x, y).channels().first().unwrap() < 200_u8 {
        if start.is_none() {
          start = Some(y);
        }
      } else if let Some(y_start) = start {
        ylines.push((x, y_start, y));
        start = None;
      }
    }
  }

  let mut xlines = Vec::new();
  for y in 0..image.height() {
    let mut start = None;

    for x in 0..image.width() {
      if *image.get_pixel(x, y).channels().first().unwrap() < 200_u8 {
        if start.is_none() {
          start = Some(x);
        }
      } else if let Some(x_start) = start {
        xlines.push((y, x_start, x));

        start = None;
      }
    }
  }

  let mut new_y_lines = Vec::new();

  for x in 0..image.width() {
    let mut lines = Vec::new();
    for (x1, start, end) in &ylines {
      if &x == x1 {
        lines.push((*start, *end));
      }
    }

    for (start, end) in clean_lines(lines, 5, (image.height() as f32 * 0.10).round() as u32) {
      new_y_lines.push((x, start, end));
    }
  }

  let mut new_x_lines = Vec::new();

  for y in 0..image.height() {
    let mut lines = Vec::new();
    for (y1, start, end) in &xlines {
      if &y == y1 {
        lines.push((*start, *end));
      }
    }

    for (start, end) in clean_lines(lines, 5, (image.width() as f32 * 0.75).round() as u32) {
      new_x_lines.push((y, start, end));
    }
  }

  let lines_y = deduplicate_lines(new_y_lines, 10, 10);
  let lines_x = deduplicate_lines(new_x_lines, 10, 10);

  let mut tess = LepTess::new(None, "deu")?;

  {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    image.write_to(&mut cursor, ImageOutputFormat::Png)?;
    tess.set_image_from_mem(&buf)?;
  }

  let mut rows = Vec::new();

  let mut last_y = None;
  for (y2, _, _) in lines_x {
    if let Some(y1) = last_y {
      last_y = None;

      let mut columns = Vec::new();

      let mut last_x = None;
      for (x2, _, _) in &lines_y {
        if let Some(x1) = last_x {
          last_x = None;

          let x3 = x1 + 4;
          let y3 = y1 + 4;

          let width3 = x2 - x1 - 8;
          let height3 = y2 - y1 - 8;

          let b = Box {
            x: x3 as i32,
            y: y3 as i32,
            w: width3 as i32,
            h: height3 as i32,
            refcount: 1,
          };
          tess.set_rectangle(std::boxed::Box::new(b));

          columns.push(
            tess.get_utf8_text()?
              .replace('\n', " ")
              .trim()
              .to_string(),
          );
          println!("{}", tess.mean_text_conf());
        } else {
          last_x = Some(*x2);
        }
      }
      rows.push(columns);
    } else {
      last_y = Some(y2);
    }
  }

  Ok(rows)
}

fn clean_lines(mut lines: Vec<(u32, u32)>, threshold: u32, min_length: u32) -> Vec<(u32, u32)> {
  let mut cleaned = Vec::new();

  if lines.is_empty() {
    return cleaned;
  }

  lines.sort_by(|(a, _), (b, _)| a.cmp(b));

  let mut min = lines[0].0;
  let mut max = lines[0].1;

  for (start, end) in &lines[1..] {
    if start - max < threshold {
      max = *end;
    } else {
      if max - min >= min_length {
        cleaned.push((min, max));
      }
      min = *start;
      max = *end;
    }
  }

  // fallback for last line
  if max - min >= min_length {
    cleaned.push((min, max));
  }

  cleaned
}

fn deduplicate_lines(
  mut lines: Vec<(u32, u32, u32)>,
  threshold: u32,
  min_distance: u32,
) -> Vec<(u32, u32, u32)> {
  let mut deduplicated = Vec::new();

  if lines.is_empty() {
    return deduplicated;
  }

  lines.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));

  let mut last_level = lines[0].0;
  let mut last_start = lines[0].1;
  let mut last_end = lines[0].2;

  for (level, start, end) in lines {
    if last_start.abs_diff(start) < threshold && last_end.abs_diff(end) < threshold {
      if level - last_level >= min_distance {
        deduplicated.push((last_level, last_start, last_end));
        deduplicated.push((level, start, end));
      }

      last_level = level;
      last_start = start;
      last_end = end;
    }
  }

  deduplicated
}
