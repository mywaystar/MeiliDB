use std::collections::hash_map::DefaultHasher;
use std::path::{Path, PathBuf};
use std::hash::{Hash, Hasher};
use std::error::Error;

use serde_derive::{Serialize, Deserialize};
use structopt::StructOpt;

use meilidb::database::schema::{Schema, SchemaBuilder, STORED, INDEXED};
use meilidb::database::update::PositiveUpdateBuilder;
use meilidb::tokenizer::DefaultBuilder;
use meilidb::database::Database;

#[derive(Debug, StructOpt)]
pub struct Opt {
    /// The destination where the database must be created
    #[structopt(parse(from_os_str))]
    pub database_path: PathBuf,

    /// The csv file to index.
    #[structopt(parse(from_os_str))]
    pub csv_data_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Document<'a> {
    id: &'a str,
    title: &'a str,
    description: &'a str,
    image: &'a str,
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn create_schema() -> Schema {
    let mut schema = SchemaBuilder::new();
    schema.new_attribute("id", STORED);
    schema.new_attribute("title", STORED | INDEXED);
    schema.new_attribute("description", STORED | INDEXED);
    schema.new_attribute("image", STORED);
    schema.build()
}

fn index(schema: Schema, database_path: &Path, csv_data_path: &Path) -> Result<Database, Box<Error>> {
    let database = Database::create(database_path, schema.clone())?;

    println!("start indexing...");

    let tokenizer_builder = DefaultBuilder::new();
    let update_path = tempfile::NamedTempFile::new()?;
    let mut update = PositiveUpdateBuilder::new(update_path.path(), schema, tokenizer_builder);

    let mut rdr = csv::Reader::from_path(csv_data_path)?;
    let mut raw_record = csv::StringRecord::new();
    let headers = rdr.headers()?.clone();

    while rdr.read_record(&mut raw_record)? {
        let document: Document = match raw_record.deserialize(Some(&headers)) {
            Ok(document) => document,
            Err(e) => {
                eprintln!("{:?}", e);
                continue;
            }
        };

        let document_id = calculate_hash(&document.id);
        update.update(document_id, &document).unwrap();
    }

    let mut update = update.build()?;

    update.set_move(true);
    database.ingest_update_file(update)?;

    Ok(database)
}

fn main() -> Result<(), Box<Error>> {
    let opt = Opt::from_args();

    let schema = create_schema();

    let (elapsed, result) = elapsed::measure_time(|| {
        index(schema, &opt.database_path, &opt.csv_data_path)
    });

    if let Err(e) = result {
        return Err(e.into())
    }

    println!("database created in {} at: {:?}", elapsed, opt.database_path);

    Ok(())
}
