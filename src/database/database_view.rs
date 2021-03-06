use std::error::Error;
use std::path::Path;
use std::ops::Deref;
use std::{fmt, marker};

use rocksdb::rocksdb_options::{ReadOptions, EnvOptions, ColumnFamilyOptions};
use rocksdb::rocksdb::{DB, DBVector, Snapshot, SeekKey, SstFileWriter};
use serde::de::DeserializeOwned;

use crate::database::{DocumentKey, DocumentKeyAttr};
use crate::database::{retrieve_data_schema, retrieve_data_index};
use crate::database::blob::positive::PositiveBlob;
use crate::database::deserializer::Deserializer;
use crate::database::schema::Schema;
use crate::rank::QueryBuilder;
use crate::DocumentId;

pub struct DatabaseView<D>
where D: Deref<Target=DB>
{
    snapshot: Snapshot<D>,
    blob: PositiveBlob,
    schema: Schema,
}

impl<D> DatabaseView<D>
where D: Deref<Target=DB>
{
    pub fn new(snapshot: Snapshot<D>) -> Result<DatabaseView<D>, Box<Error>> {
        let schema = retrieve_data_schema(&snapshot)?;
        let blob = retrieve_data_index(&snapshot)?;
        Ok(DatabaseView { snapshot, blob, schema })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn blob(&self) -> &PositiveBlob {
        &self.blob
    }

    pub fn into_snapshot(self) -> Snapshot<D> {
        self.snapshot
    }

    pub fn snapshot(&self) -> &Snapshot<D> {
        &self.snapshot
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<DBVector>, Box<Error>> {
        Ok(self.snapshot.get(key)?)
    }

    pub fn dump_all<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<Error>> {
        let path = path.as_ref().to_string_lossy();

        let env_options = EnvOptions::new();
        let column_family_options = ColumnFamilyOptions::new();
        let mut file_writer = SstFileWriter::new(env_options, column_family_options);
        file_writer.open(&path)?;

        let mut iter = self.snapshot.iter();
        iter.seek(SeekKey::Start);

        for (key, value) in &mut iter {
            file_writer.put(&key, &value)?;
        }

        file_writer.finish()?;
        Ok(())
    }

    pub fn query_builder(&self) -> Result<QueryBuilder<D>, Box<Error>> {
        QueryBuilder::new(self)
    }

    // TODO create an enum error type
    pub fn retrieve_document<T>(&self, id: DocumentId) -> Result<T, Box<Error>>
    where T: DeserializeOwned
    {
        let mut deserializer = Deserializer::new(&self.snapshot, &self.schema, id);
        Ok(T::deserialize(&mut deserializer)?)
    }

    pub fn retrieve_documents<T, I>(&self, ids: I) -> DocumentIter<D, T, I::IntoIter>
    where T: DeserializeOwned,
          I: IntoIterator<Item=DocumentId>,
    {
        DocumentIter {
            database_view: self,
            document_ids: ids.into_iter(),
            _phantom: marker::PhantomData,
        }
    }
}

impl<D> fmt::Debug for DatabaseView<D>
where D: Deref<Target=DB>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut options = ReadOptions::new();
        let lower = DocumentKey::new(0);
        options.set_iterate_lower_bound(lower.as_ref());

        let mut iter = self.snapshot.iter_opt(options);
        iter.seek(SeekKey::Start);
        let iter = iter.map(|(key, _)| DocumentKeyAttr::from_bytes(&key));

        if f.alternate() {
            writeln!(f, "DatabaseView(")?;
        } else {
            write!(f, "DatabaseView(")?;
        }

        self.schema.fmt(f)?;

        if f.alternate() {
            writeln!(f, ",")?;
        } else {
            write!(f, ", ")?;
        }

        f.debug_list().entries(iter).finish()?;

        write!(f, ")")
    }
}

// TODO this is just an iter::Map !!!
pub struct DocumentIter<'a, D, T, I>
where D: Deref<Target=DB>
{
    database_view: &'a DatabaseView<D>,
    document_ids: I,
    _phantom: marker::PhantomData<T>,
}

impl<'a, D, T, I> Iterator for DocumentIter<'a, D, T, I>
where D: Deref<Target=DB>,
      T: DeserializeOwned,
      I: Iterator<Item=DocumentId>,
{
    type Item = Result<T, Box<Error>>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.document_ids.size_hint()
    }

    fn next(&mut self) -> Option<Self::Item> {
        match self.document_ids.next() {
            Some(id) => Some(self.database_view.retrieve_document(id)),
            None => None
        }
    }
}

impl<'a, D, T, I> ExactSizeIterator for DocumentIter<'a, D, T, I>
where D: Deref<Target=DB>,
      T: DeserializeOwned,
      I: ExactSizeIterator + Iterator<Item=DocumentId>,
{ }

impl<'a, D, T, I> DoubleEndedIterator for DocumentIter<'a, D, T, I>
where D: Deref<Target=DB>,
      T: DeserializeOwned,
      I: DoubleEndedIterator + Iterator<Item=DocumentId>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        match self.document_ids.next_back() {
            Some(id) => Some(self.database_view.retrieve_document(id)),
            None => None
        }
    }
}
