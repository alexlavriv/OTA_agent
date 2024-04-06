import unittest
from linux import test_base

if __name__ == '__main__':
    suite = unittest.TestLoader().discover('.', pattern="test*")
    unittest.TextTestRunner(verbosity=2).run(suite)
