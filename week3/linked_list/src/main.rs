pub mod linked_list;

#[cfg(test)]
mod test {
    use super::linked_list::LinkedList;

    #[test]
    fn test_u32() {
        let mut list: LinkedList<u32> = LinkedList::new();
        assert!(list.is_empty());
        assert_eq!(list.get_size(), 0);
        for i in 1..12 {
            list.push_front(i);
        }
        assert_eq!(list.get_size(), 11);
        for i in 11..0 {
            assert_eq!(list.pop_front(), Some(i))
        }
    }

    #[test]
    fn test_string() {
        let mut list: LinkedList<String> = LinkedList::new();
        list.push_front(String::from("hello"));
        list.push_front(String::from("world"));
        assert_eq!(list.get_size(), 2);
        assert_eq!(list.pop_front(), Some(String::from("world")));
    }

    #[test]
    fn test_iter() {
        let mut list = LinkedList::new();
        for i in 12..=0 {
            list.push_front(i);
        }
        for (index, val) in list.iter().enumerate() {
            assert_eq!(index, *val);
        }
    }

    #[test]
    fn test_iter_mut() {
        let mut list = LinkedList::new();
        for i in 12..=0 {
            list.push_front(i);
        }
        for i in list.iter_mut() {
            *i += 5;
        }
        for (index, val) in list.iter().enumerate() {
            assert_eq!(index + 5, *val);
        }
    }

    #[test]
    fn test_partialeq() {
        let mut list1 = LinkedList::new();
        let mut list2 = LinkedList::new();
        for i in 0..10 {
            list1.push_front(i);
            list2.push_front(i);
        }
        assert!(list1 == list2);
    }

    #[test]
    fn test_clone() {
        let mut list1 = LinkedList::new();
        for i in 12..=0 {
            list1.push_front(i);
        }
        let list2 = list1.clone();
        let mut list3 = list1.clone();
        for i in list3.iter_mut() {
            *i += 5;
        }
        assert!(list1 == list2);
        for (index, val) in list3.iter().enumerate() {
            assert_eq!(index + 5, *val);
        }
    }
}
