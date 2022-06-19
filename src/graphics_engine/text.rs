use super::textures::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use regex::*;
use super::drawable::*;
use VertexSimple as Vertex;
use glium::*;
use super::shader;
use super::instancing::*;
use std::rc::Rc;
use crate::cg_support::{node::Node, Transformation};
use std::cell::RefCell;
use super::entity::*;

const RECT_VERTS : [Vertex; 4] = [
    Vertex { pos: [1., 1., 0.], tex_coords: [1., 1.]},
    Vertex { pos: [-1., 1., 0.], tex_coords: [0., 1.]},
    Vertex { pos: [-1., -1., 0.], tex_coords: [0., 0.]},
    Vertex { pos: [1., -1., 0.], tex_coords: [1., 0.]}
];

const RECT_INDICES : [u32; 6] = [0, 1, 3, 3, 1, 2];

struct Glyph {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    advance: i32,
    xoff: i32,
    _yoff: i32,
}

pub struct Font {
    glyphs: HashMap<u8, Glyph>,
    kernings: HashMap<u8, HashMap<u8, i32>>,
    sdf: glium::texture::Texture2d,
    _line_height: i32,
    img_width: i32,
    img_height: i32,
}

impl Font {
    fn parse_line_to_glyph(line: &str, param_regex: &Regex) -> Option<(u8, Glyph)> {
        let mut x : Option<i32> = None;
        let mut y : Option<i32> = None;
        let mut height : Option<i32> = None;
        let mut width : Option<i32> = None;
        let mut advance : Option<i32> = None;
        let mut xoff : Option<i32> = None;
        let mut yoff : Option<i32> = None;
        let mut char_id : Option<i32> = None;
        for cap in param_regex.captures_iter(line.trim()) {
            let val = cap.get(2).expect("Unable to get numeric capture group")
                .as_str().parse::<i32>().expect("Unable to parse value as i32");
            let key = cap.get(1).expect("Unable to get label capture group")
                .as_str();

            match key {
                "x" => x = Some(val),
                "y" => y = Some(val),
                "height" => height = Some(val),
                "width" => width = Some(val),
                "xadvance" => advance = Some(val),
                "xoffset" => xoff = Some(val),
                "yoffset" => yoff = Some(val),
                "char id" => char_id = Some(val),
                _ => (),
            }
        }
        if let Some(char_id) = char_id {
            Some((char_id as u8, Glyph {
                x: x.unwrap(), y: y.unwrap(), 
                height: height.unwrap(), width: width.unwrap(), 
                advance: advance.unwrap(), xoff: xoff.unwrap(), 
                _yoff: yoff.unwrap()
            }))
        } else { None }
    }

    fn parse_line_to_kerning(line: &str, regex: &Regex) -> Option<(u8, (u8, i32))> {
        let mut first : Option<u8> = None;
        let mut second : Option<u8> = None;
        let mut amount : Option<i32> = None;
        for cap in regex.captures_iter(line.trim()) {
            let val = cap.get(2).expect("Unable to get numeric capture group")
                .as_str().parse::<i32>().expect("Unable to parse value as i32");
            let key = cap.get(1).expect("Unable to get label capture group")
                .as_str();

            match key {
                "first" => first = Some(val as u8),
                "second" => second = Some(val as u8),
                "amount" => amount = Some(val),
                _ => (),
            }
        }

        if first.is_some() {
            Some((first.unwrap(), (second.unwrap(), amount.unwrap())))
        } else { None }
    }

    /// Gets the line height, image width, image height, and texture path from 
    /// the header of the font file
    fn read_from_header(header: &str) -> (i32, i32, i32, String) {
        let get_integral_field = |key| {
            Regex::new(&format!("{}=([0-9]+)", key)).unwrap()
            .captures(header).expect(&format!("No matching pattern found for {}", key))
            .get(1).expect("No capture group at index 1").as_str().parse::<i32>()
            .expect("Could not parse line height to an integer")
        };
        let line_height = get_integral_field("lineHeight");
        let width = get_integral_field("scaleW");
        let height = get_integral_field("scaleH");
        let tex_path = Regex::new(r#"file="([a-zA-Z\.]+)""#).unwrap().captures(header)
            .expect("No matching file pattern found").get(1)
            .expect("Unable to get file path capture group").as_str();
        (line_height, width, height, tex_path.to_owned())
    }

    /// `path` - path to the `fnt` file which contains the textual metadata
    pub fn new<F : backend::Facade>(path: &str, f: &F) -> Font {
        let dir = dir_stem(path);
        let mut file = File::open(path).expect(
            &format!("Could not open font file: {}", path));
        let mut data = String::new();
        file.read_to_string(&mut data).expect("Could not read from font file");
        let (header, content) = data.split_at(data.find("chars count").unwrap());
        let (char_data, kerning_data) = 
            content.split_at(content.find("kernings count").unwrap());
        let (_line_height, img_width, img_height, tex_path) = 
            Self::read_from_header(header);

        let rg_param = Regex::new(r#"([a-z][a-z\s]*)=(-?[0-9]+)"#).unwrap();
        let mut glyphs = HashMap::new();
        let mut kernings = HashMap::new();
        for line in char_data.split('\n') {
            if let Some((k, v)) = Self::parse_line_to_glyph(line, &rg_param) {
                glyphs.insert(k, v);
            }
        }
        for line in kerning_data.split('\n') {
            if let Some((first, (second, amount))) = 
                Self::parse_line_to_kerning(line, &rg_param) 
            {
                kernings.entry(first).or_insert(HashMap::new())
                    .insert(second, amount);
            }
        }

        let sdf = load_texture_2d(&format!("{}/{}", dir, tex_path), f);
        Font {
            _line_height, glyphs, sdf,
            img_width, img_height, kernings
        }
    }
}

pub struct Text {
    vertices: VertexBuffer<Vertex>,
    indicies: IndexBuffer<u32>,
    instances: InstanceBuffer<TextAttributes>,
    instance_pos: InstanceBuffer<InstancePosition>,
    font: Rc<Font>,
    attribs: Vec<TextAttributes>,
    positions: Vec<Node>,
    dirty: bool,
}

impl Text {
    pub fn new<F : backend::Facade>(font: Rc<Font>, facade: &F) -> Text {
        Text {
            vertices: VertexBuffer::new(facade, &RECT_VERTS).unwrap(),
            indicies: IndexBuffer::new(facade, 
                glium::index::PrimitiveType::TrianglesList, &RECT_INDICES)
                .unwrap(),
            instances: InstanceBuffer::new(),
            instance_pos: InstanceBuffer::new(),
            font,
            attribs: Vec::new(),
            positions: Vec::new(),
            dirty: false,
        }
    }

    /// Adds an instance of text with the given string, position/scaling, and color
    pub fn add_text(&mut self, txt: &str, pos: Rc<RefCell<Node>>, color: [f32; 4]) {
        use cgmath::*;
        let mut last_x = 0;
        let fnt = self.font.clone();
        let mut last_char : u8 = 0;
        for (g, c) in txt.as_bytes().iter()
            .filter_map(|c| fnt.glyphs.get(c).map(|g| (g, c)))
        {
            let offset = 
                if let Some(offsets) = fnt.kernings.get(&last_char) {
                    offsets.get(c).map(|e| *e).unwrap_or(0)
                } else { 0 };
            let pt = pos.borrow().transform_pt(
                point3((last_x + offset) as f64, 0., 0.));
            let p = Node::default().parent(pos.clone()).pos(pt);
            last_x += g.advance.min(9);
            self.positions.push(p);
            self.attribs.push(TextAttributes {
                x_y_width_height: [g.x, g.y, g.width, g.height],
                color,
            });
            last_char = *c;
        }
        self.dirty = true;
    }
}

impl Drawable for Text {
    fn render_args<'a>(&'a mut self, _positions: &[[[f32; 4]; 4]]) 
    -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        if self.dirty {
            // TODO: will not work if we want to move text after adding it
            // by changing the parent's transformation node
            let ctx = super::super::get_active_ctx();
            let ctx = ctx.ctx.borrow();
            self.instances.update_buffer(&self.attribs, &*ctx);
            let ps : Vec<_> = self.positions.iter().map(
                |x| mat_to_instance_pos(&x.mat())).collect();
            self.instance_pos.update_buffer(&ps, &*ctx);
            self.dirty = false;
        }

        let attribs : glium::vertex::VerticesSource<'a> 
            = From::from(self.instances.get_stored_buffer().unwrap()
                .per_instance().unwrap());
        let locs : glium::vertex::VerticesSource<'a> 
            = From::from(self.instance_pos.get_stored_buffer().unwrap()
                .per_instance().unwrap());
        let vertices = VertexHolder::new(VertexSourceData::Single(
            From::from(&self.vertices))).append(locs).append(attribs);
        vec![(shader::UniformInfo::TextInfo(&self.font.sdf, 
            [self.font.img_width, self.font.img_height]), vertices, 
            From::from(&self.indicies))]
    }

    fn transparency(&self) -> Option<f32> { None }
}

impl AbstractEntity for Text {
    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        None
    }

    fn drawable(&mut self) -> &mut dyn Drawable {
        self
    }

    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        match pass {
            shader::RenderPassType::Visual => true,
            _ => false,
        }

    }


    fn render_order(&self) -> RenderOrder {
        RenderOrder::Unordered
    }
}
