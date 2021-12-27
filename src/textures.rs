use glium::Surface;

/// Loads a 16 bit RGB texture
fn load_hdr(path: &str) -> glium::texture::RawImage2d<u16> {
    //let img = image::codecs::hdr::read_raw_file(path).unwrap();
    let f = std::fs::File::open(path).expect(&format!("Could not load '{}'", path));
    let img = image::load(std::io::BufReader::new(f),
        image::ImageFormat::from_path(path).unwrap()).unwrap().to_rgb16();
    let dims = img.dimensions();
    glium::texture::RawImage2d::from_raw_rgb_reversed(&img.into_raw(), dims)
}

/// Loads an 8bit RGBA image from `path`, reversing it on load if `rev` is `true`
fn load_img(path: &str, rev: bool) -> glium::texture::RawImage2d<u8> {
    let f = std::fs::File::open(path).expect(&format!("Could not load '{}'", path));
    let img = image::load(std::io::BufReader::new(f), 
        image::ImageFormat::from_path(path).unwrap()).unwrap().to_rgba8();
    let dims = img.dimensions();
    if rev {
        glium::texture::RawImage2d::from_raw_rgba_reversed(&img.into_raw(), dims)
    } else {
        glium::texture::RawImage2d::from_raw_rgba(img.into_raw(), dims)
    }
}

/// Loads an sRGB texture as 8bit RGBA with mipmaps from `path`. The image is reversed on load
pub fn load_texture_srgb<F : glium::backend::Facade>(path: &str, facade: &F) 
    -> glium::texture::SrgbTexture2d
{
    let tex = load_img(path, true);
    glium::texture::SrgbTexture2d::with_mipmaps(facade, tex,
        glium::texture::MipmapsOption::AutoGeneratedMipmaps).unwrap()
}

/// Loads a texture as 8bit RGBA with mipmaps. Image is reversed
pub fn load_texture_2d<F : glium::backend::Facade>(path: &str, facade: &F) 
    -> glium::texture::Texture2d 
{
    let tex = load_img(path, false);
    glium::Texture2d::with_mipmaps(facade, tex, 
        glium::texture::MipmapsOption::AutoGeneratedMipmaps).unwrap()
}

/// Loads a texture with 16bit RGB with mipmaps. Image is reversed
pub fn load_texture_hdr<F : glium::backend::Facade>(path: &str, facade: &F)
    -> glium::texture::Texture2d
{
    glium::Texture2d::new(facade, load_hdr(path)).unwrap()
}

/// Gets the directory from `path`
pub fn dir_stem(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => format!("{}/", path.split_at(idx).0),
        _ => String::new(),
    }  
    
}

pub fn load_tex_srgb_or_empty<F : glium::backend::Facade>(path: &str, facade: &F)
    -> glium::texture::SrgbTexture2d 
{
    if path.is_empty() || path.rfind('.').is_none() {
        glium::texture::SrgbTexture2d::empty(facade, 0, 0).unwrap()
    } else {
        load_texture_srgb(path, facade)
    }
}

pub fn load_tex_2d_or_empty<F : glium::backend::Facade>(path: &str, facade: &F)
    -> glium::texture::Texture2d 
{
    if path.is_empty() || path.rfind('.').is_none() {
        glium::texture::Texture2d::empty(facade, 0, 0).unwrap()
    } else {
        load_texture_2d(path, facade)
    }
}

pub fn load_cubemap<F>(file: &str, facade: &F) 
    -> glium::texture::Cubemap where F : glium::backend::Facade 
{
    use glium::texture::CubeLayer;
    let dir = dir_stem(file);
    let extension = file.split_at(file.rfind('.').unwrap()).1;
    let faces = [("right", CubeLayer::PositiveX), ("left", CubeLayer::NegativeX),
        ("top", CubeLayer::PositiveY), ("bottom", CubeLayer::NegativeY), ("front", CubeLayer::PositiveZ),
        ("back", CubeLayer::NegativeZ)];
    let im_size = 2048;
    let cubemap = glium::texture::Cubemap::empty(facade, im_size).unwrap();
    let dst_target = glium::BlitTarget {
        left: 0,
        bottom: 0,
        width: im_size as i32,
        height: im_size as i32,
    };
    for (name, cube_layer) in faces {
        let fbo = glium::framebuffer::SimpleFrameBuffer::new(facade, 
            cubemap.main_level().image(cube_layer)).unwrap();
        let img = load_texture_2d(&format!("{}{}{}", dir, name, extension), facade);
        img.as_surface().blit_whole_color_to(&fbo, &dst_target,
            glium::uniforms::MagnifySamplerFilter::Linear);
    }
    cubemap
}