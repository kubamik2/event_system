use std::{any::{Any, TypeId}, collections::HashMap};

#[derive(Default)]
pub struct ShareTypeMap(HashMap<TypeId, Box<dyn Any + Send + Sync>>);

impl ShareTypeMap {
    #[inline]
    pub fn get<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>()).map(|f| unsafe { f.downcast_ref().unwrap_unchecked() } )
    }

    #[inline]
    pub fn get_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>()).map(|f| unsafe { f.downcast_mut().unwrap_unchecked() } )
    }

    #[inline]
    pub fn insert<T: 'static + Send + Sync>(&mut self, item: T) -> Option<T> {
        let res = self.0.insert(item.type_id(), Box::new(item));
        res.map(|f| *unsafe { f.downcast::<T>().unwrap_unchecked() })
    }

    #[inline]
    pub fn remove<T: 'static + Send + Sync>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())
            .map(|f| *unsafe { f.downcast::<T>().unwrap_unchecked() })
    }
}
