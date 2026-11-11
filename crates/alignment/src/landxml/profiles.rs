use super::{Node, append, escaped, number};
use crate::{
    Alignment, MAX_ELEMENTS, MAX_POINTS, Result, error,
    profile::{DesignProfile, GroundProfile, Profiles, Pvi, Sample},
};

pub(super) fn parse(n: &Node, a: &Alignment, scale: f64) -> Result<Profiles> {
    let mut out = Profiles::default();
    for profile in n.children.iter().filter(|n| n.name == "Profile") {
        for child in &profile.children {
            match child.name.as_str() {
                "ProfAlign" => {
                    if out.design.is_some() {
                        return Err(error("UNSUPPORTED_PROFILE", "multiple design profiles cannot be stored on one carrier"));
                    }
                    out.design = Some(design(child, scale)?);
                }
                "ProfSurf" => {
                    if out.ground.is_some() {
                        return Err(error("UNSUPPORTED_PROFILE", "multiple ground profiles cannot be stored on one carrier"));
                    }
                    out.ground = Some(ground(child, scale)?);
                }
                "Feature" => {}
                _ => return Err(error("UNSUPPORTED_VERTICAL_CURVE", format!("unsupported profile element {}", child.name))),
            }
        }
    }
    out.validate(a)?;
    Ok(out)
}
fn pair(n: &Node, scale: f64) -> Result<(f64, f64)> {
    let mut tokens = n.text.split_whitespace();
    let station = number(tokens.next().ok_or_else(|| error("INVALID_PROFILE", "missing station"))?)? * scale;
    let elevation = number(tokens.next().ok_or_else(|| error("INVALID_PROFILE", "missing elevation"))?)? * scale;
    if tokens.next().is_some() || !n.children.is_empty() {
        return Err(error("INVALID_PROFILE", "station elevation pair required"));
    }
    Ok((station, elevation))
}
fn design(n: &Node, scale: f64) -> Result<DesignProfile> {
    let mut pvis = Vec::new();
    for child in &n.children {
        if child.name == "Feature" {
            continue;
        }
        if !matches!(child.name.as_str(), "PVI" | "ParaCurve") {
            return Err(error("UNSUPPORTED_VERTICAL_CURVE", format!("{}: only symmetric ParaCurve/PVI supported", child.name)));
        }
        if pvis.len() >= MAX_ELEMENTS {
            return Err(error("POINT_LIMIT", "more than 4096 PVIs"));
        }
        let (station, elevation) = pair(child, scale)?;
        pvis.push(Pvi { station, elevation, curve_length: if child.name == "ParaCurve" { Some(child.number("length")? * scale) } else { None } });
    }
    Ok(DesignProfile { name: n.attr("name")?.into(), pvis })
}
fn ground(n: &Node, scale: f64) -> Result<GroundProfile> {
    let mut segments = Vec::new();
    let mut count = 0usize;
    for child in &n.children {
        if child.name == "Feature" {
            continue;
        }
        if child.name != "PntList2D" || !child.children.is_empty() {
            return Err(error("UNSUPPORTED_PROFILE", "ProfSurf requires PntList2D"));
        }
        let mut tokens = child.text.split_whitespace();
        let mut samples = Vec::new();
        while let Some(station) = tokens.next() {
            if count >= MAX_POINTS {
                return Err(error("POINT_LIMIT", "ground profile exceeds 100000 samples"));
            }
            count += 1;
            let elevation = tokens.next().ok_or_else(|| error("INVALID_PROFILE", "odd PntList2D token count"))?;
            samples.push(Sample { station: number(station)? * scale, elevation: number(elevation)? * scale });
        }
        segments.push(samples);
    }
    Ok(GroundProfile { name: n.attr("name")?.into(), segments })
}
pub(super) fn write(out: &mut String, a: &Alignment, p: &Profiles) -> Result<()> {
    if p.design.is_none() && p.ground.is_none() {
        return Ok(());
    }
    append(out, &format!("<Profile staStart=\"{}\">", a.start_station))?;
    if let Some(d) = &p.design {
        append(out, &format!("<ProfAlign name=\"{}\">", escaped(&d.name)))?;
        for p in &d.pvis {
            match p.curve_length {
                Some(length) => append(out, &format!("<ParaCurve length=\"{length}\">{} {}</ParaCurve>", p.station, p.elevation))?,
                None => append(out, &format!("<PVI>{} {}</PVI>", p.station, p.elevation))?,
            }
        }
        append(out, "</ProfAlign>")?;
    }
    if let Some(g) = &p.ground {
        append(out, &format!("<ProfSurf name=\"{}\">", escaped(&g.name)))?;
        for segment in &g.segments {
            append(out, "<PntList2D>")?;
            for point in segment {
                append(out, &format!("{} {} ", point.station, point.elevation))?;
            }
            append(out, "</PntList2D>")?;
        }
        append(out, "</ProfSurf>")?;
    }
    append(out, "</Profile>")
}
