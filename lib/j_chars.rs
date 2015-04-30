#[derive(Debug)]
pub struct JavaChars {
	contents: Vec<u8>
}

impl JavaChars {
	pub fn new(value: &str) -> JavaChars {
		let mut vector:Vec<u8> = vec![];
		for i in value.chars() {
			let j:i32 = i as i32;
			match i {
				'\0' | '\u{80}' ... '\u{7FF}' => {
					vector.push(b'\xC0' | (j >> 6) as u8)
						; vector.push(0x80|(0x3F & j) as u8)
				}
				'\u{1}' ... '\u{7F}' => vector.push(i as u8),
				'\u{800}' ... '\u{FFFF}' => {
					vector.push(b'\xE0' | (j >> 12) as u8)
						; vector.push(0x80 | (0x3F & (j >> 6)) as u8)
						; vector.push(0x80 | (0x3F & j) as u8)
				}
				'\u{10000}' ... '\u{10FFFF}' => {
					let subchar = j - 0x10000;
					vector.push(0b11101101);
					vector.push(0b10100000 | (subchar >> 16) as u8);
					vector.push(0b10000000 | (0x3F & (subchar >> 10)) as u8);
					vector.push(0b11101101);
					vector.push(0b11101101);
					vector.push(0b10100000 | (0x0F & (subchar >> 6)) as u8);
					vector.push(0b10000000 | (0x3F & subchar) as u8);
				}
				_ => unreachable!()
			}
		}
		vector.push(b'\0');
		JavaChars { contents: vector, }
	}
	pub fn to_string(&self) -> Option<String> {
		let mut a = Vec::<u8>::with_capacity(self.contents.len());
		let mut counter:usize = 0;
		loop {
			let i = self.contents[counter];
			counter += 1;
			match i {
				b'\x00' => unsafe {
					return Some(String::from_utf8_unchecked(a))
				},
				b'\x01' ... b'\x7F' => a.push(i),
				b'\xC0' => {
					let j = self.contents[counter];
					counter += 1;
					if j == b'\x80' {
						a.push(b'\0');
					} else {
						a.push(i);
						a.push(j)
					}
				}
				b'\xC1' ... b'\xDF' => {
					a.push(i);
					a.push(self.contents[counter]);
					counter += 1
				}
				b'\xED' => {
					let next_char: u8 = self.contents[counter];
					counter += 1;
					if next_char & 0xE0 == 0xA0 {
						// Surrogate char
						if next_char & 0x10 != 0 {
							// Invalid surrogate
							return None
						} else {
							// The high byte
							let high_byte = 0xF1 + (next_char >> 2);

							let next_char2:u8 =
								self.contents[counter] & 0x0F;
							counter += 1;

							let second_byte = (next_char2 >> 2) |
								((high_byte & 0x3) << 4) | 0x80;
							if self.contents[counter] != b'\xED' {
								// Not a surrogate
								return None
							}
							counter += 1;

							let next_char3 = 0x3F | self.contents[counter];
							if next_char3 & 0xF0 != 0xB0 {
								// Invalid high surrogate
								return None
							}
							counter += 1;

							let third_byte =
								next_char3|((next_char2&0x3)<<4);

							let next_char4 = self.contents[counter];
							counter += 1;
							a.push(high_byte);
							a.push(second_byte);
							a.push(third_byte);
							a.push(next_char4)
						}
					} else {
						a.push(i);
						a.push(next_char);
						a.push(self.contents[counter]);
						counter += 1
					}
				}
				b'\xE0' ... b'\xEF' => {
					a.push(i);
					a.push(self.contents[counter]); counter += 1;
					a.push(self.contents[counter]); counter += 1
				}
				_ => panic!("Invalid Java string")
			}
		}
	}
	pub fn as_vec(&self) -> &Vec<u8> {
		&(self.contents)
	}
	pub fn as_ptr(&self) -> *const ::libc::c_char {
		self.contents.as_ptr() as *const ::libc::c_char
	}
	pub unsafe fn from_raw_vec (data: Vec<u8>) -> Self {
		JavaChars { contents: data, }
	}
}
// vim: set noexpandtab:
// vim: set tabstop=4:
// vim: set shiftwidth=4:
// Local Variables:
// mode: rust
// indent-tabs-mode: t
// rust-indent-offset: 4
// tab-width: 4
// End:
