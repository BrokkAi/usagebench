package example;

public class ServiceTest {
    public void runsService() {
        Service.Repository repository = new Service.Repository();
        Service service = new Service(repository);
        String result = service.execute(" Ada ");
        System.out.println(Service.DEFAULT_PREFIX + result);
    }
}
